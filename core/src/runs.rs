//! Progress through runs, and placing runs before or after steps of others.

use crate::types::{Mark, Node, NodeRef, Rel, Run};
use indexmap::{Equivalent, IndexMap};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Runs by id, in the order they were stored.
pub type Runs = IndexMap<String, Run>;

/// Extra prerequisites placing creates: (run id, step id) to the steps of
/// other runs that step waits on, as [`cross_links`] returns them.
pub type Links = IndexMap<(String, String), Vec<NodeRef>>;

/// A (run id, step id) key that looks up [`Links`] without allocating. It
/// hashes as the `(String, String)` it stands for.
#[derive(Hash)]
struct Key<'k>(&'k str, &'k str);

impl Equivalent<(String, String)> for Key<'_> {
    fn equivalent(&self, k: &(String, String)) -> bool {
        self.0 == k.0 && self.1 == k.1
    }
}

/// Each run, with its steps by id (the first step with an id wins).
type Index<'a> = HashMap<&'a str, (&'a Run, HashMap<&'a str, &'a Node>)>;

fn index(runs: &Runs) -> Index<'_> {
    runs.iter()
        .map(|(id, r)| {
            let mut nodes = HashMap::new();
            for n in &r.nodes {
                nodes.entry(n.id.as_str()).or_insert(n);
            }
            (id.as_str(), (r, nodes))
        })
        .collect()
}

/// What [`ready`] and [`waiting`] need to see placements: all runs and the
/// links between them, built together so they cannot disagree. Without it
/// only a run's own steps count.
#[derive(Debug)]
pub struct Ctx<'a> {
    links: Links,
    nodes: Index<'a>,
}

impl<'a> Ctx<'a> {
    pub fn new(runs: &'a Runs) -> Self {
        Ctx {
            links: cross_links(runs),
            nodes: index(runs),
        }
    }
}

/// A step still blocked, and by what.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waiting {
    #[serde(flatten)]
    pub node: Node,
    /// Unresolved steps of its own run it waits on.
    #[serde(rename = "blockedBy")]
    pub blocked_by: Vec<String>,
    /// Unresolved steps of other runs it waits on.
    #[serde(rename = "blockedExt")]
    pub blocked_ext: Vec<NodeRef>,
}

/// Done or skipped.
fn resolved(run: &Run, id: &str) -> bool {
    matches!(run.done.get(id), Some(Mark::Done | Mark::Skipped))
}

/// Steps that wait on nothing inside the run.
pub fn roots(run: &Run) -> Vec<&Node> {
    run.nodes.iter().filter(|n| n.deps.is_empty()).collect()
}

/// Steps nothing inside the run waits on.
pub fn sinks(run: &Run) -> Vec<&Node> {
    let used: std::collections::HashSet<&str> = run
        .nodes
        .iter()
        .flat_map(|n| n.deps.iter().map(String::as_str))
        .collect();
    run.nodes
        .iter()
        .filter(|n| !used.contains(n.id.as_str()))
        .collect()
}

/// Cross-run waiting created by placing runs (`run.attach`):
/// - after: the run's first steps wait on that step;
/// - before: that step waits on the run's last steps.
///
/// A placement relative to a run that no longer exists is ignored.
pub fn cross_links(runs: &Runs) -> Links {
    let mut map: Links = IndexMap::new();
    for r in runs.values() {
        let Some(a) = &r.attach else { continue };
        if !runs.contains_key(&a.run) {
            continue;
        }
        match a.rel {
            Rel::After => {
                for n in roots(r) {
                    map.entry((r.id.clone(), n.id.clone()))
                        .or_default()
                        .push(NodeRef {
                            run: a.run.clone(),
                            id: a.id.clone(),
                        });
                }
            }
            Rel::Before => {
                for n in sinks(r) {
                    map.entry((a.run.clone(), a.id.clone()))
                        .or_default()
                        .push(NodeRef {
                            run: r.id.clone(),
                            id: n.id.clone(),
                        });
                }
            }
        }
    }
    map
}

/// Unresolved steps of other runs that step `n` of `run` waits on.
fn ext_blockers<'c>(
    run: &Run,
    n: &Node,
    ctx: Option<&'c Ctx<'_>>,
) -> impl Iterator<Item = &'c NodeRef> {
    ctx.and_then(|c| {
        let links = c.links.get(&Key(&run.id, &n.id))?;
        Some(links.iter().filter(move |x| !ext_resolved(&c.nodes, x)))
    })
    .into_iter()
    .flatten()
}

/// A step of another run counts as resolved once it is done or skipped, and
/// also when its run or the step no longer exists.
fn ext_resolved(nodes: &Index<'_>, x: &NodeRef) -> bool {
    match nodes.get(x.run.as_str()) {
        None => true,
        Some((r, steps)) => !steps.contains_key(x.id.as_str()) || resolved(r, &x.id),
    }
}

fn is_ready(run: &Run, n: &Node, ctx: Option<&Ctx<'_>>) -> bool {
    n.deps.iter().all(|d| resolved(run, d)) && ext_blockers(run, n, ctx).next().is_none()
}

/// Steps you can do right now.
pub fn ready<'r>(run: &'r Run, ctx: Option<&Ctx<'_>>) -> Vec<&'r Node> {
    run.nodes
        .iter()
        .filter(|n| !resolved(run, &n.id) && is_ready(run, n, ctx))
        .collect()
}

/// Steps still blocked, with what blocks them.
pub fn waiting(run: &Run, ctx: Option<&Ctx<'_>>) -> Vec<Waiting> {
    run.nodes
        .iter()
        .filter(|n| !resolved(run, &n.id) && !is_ready(run, n, ctx))
        .map(|n| Waiting {
            node: n.clone(),
            blocked_by: n
                .deps
                .iter()
                .filter(|d| !resolved(run, d))
                .cloned()
                .collect(),
            blocked_ext: ext_blockers(run, n, ctx).cloned().collect(),
        })
        .collect()
}

/// Every step is done or skipped. A run with no steps counts as finished.
pub fn is_finished(run: &Run) -> bool {
    run.nodes.iter().all(|n| resolved(run, &n.id))
}

/// Would the current placements make steps across runs wait on each other in
/// a loop?
pub fn has_cycle(runs: &Runs) -> bool {
    let links = cross_links(runs);
    let nodes = index(runs);
    let mut state = HashMap::new();
    runs.values().any(|r| {
        r.nodes
            .iter()
            .any(|n| visit(&nodes, &links, &mut state, &r.id, &n.id))
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Visit {
    InProgress,
    Done,
}

fn visit<'a>(
    nodes: &Index<'a>,
    links: &'a Links,
    state: &mut HashMap<(&'a str, &'a str), Visit>,
    run_id: &'a str,
    node_id: &'a str,
) -> bool {
    let k = (run_id, node_id);
    match state.get(&k) {
        Some(Visit::Done) => return false,
        Some(Visit::InProgress) => return true,
        None => {}
    }
    state.insert(k, Visit::InProgress);
    let own = nodes
        .get(run_id)
        .and_then(|(_, steps)| steps.get(node_id))
        .map(|n| n.deps.as_slice())
        .unwrap_or_default();
    let ext = links
        .get(&Key(run_id, node_id))
        .map_or(&[][..], Vec::as_slice);
    let deps = own
        .iter()
        .map(|d| (run_id, d.as_str()))
        .chain(ext.iter().map(|x| (x.run.as_str(), x.id.as_str())));
    let mut cyc = false;
    for (r, id) in deps {
        if nodes.contains_key(r) && visit(nodes, links, state, r, id) {
            cyc = true;
            break;
        }
    }
    state.insert(k, Visit::Done);
    cyc
}
