//! Read-only answers about the data, shaped for a screen or an MCP tool:
//! what to do now, what is coming up, what is in the backlog.

use crate::change::live_runs;
use crate::runs::{Ctx, ready, waiting};
use crate::types::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A step of a running run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TodayStep {
    pub run: String,
    /// The run's name (the to-do's title for a to-do).
    pub run_name: String,
    pub step: String,
    pub title: String,
    /// The routine the step came from, when it is a sub-routine's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Steps it still waits on, in its own run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
    /// Steps of other runs it still waits on (from placing).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after_other: Vec<NodeRef>,
}

/// A run that is not finished.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunningRun {
    pub run: String,
    pub name: String,
    /// A to-do pulled from the backlog, rather than a routine.
    pub todo: bool,
    /// Options that are on.
    pub flags: Vec<String>,
    pub done: usize,
    pub total: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placed: Option<Attach>,
}

/// Today: every unfinished run, the steps that can be done now and the ones
/// still waiting, across runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Today {
    pub do_now: Vec<TodayStep>,
    pub coming_up: Vec<TodayStep>,
    pub running: Vec<RunningRun>,
}

/// Today, as the Today screen shows it: runs in the order they started.
pub fn today(db: &Db) -> Today {
    let runs = live_runs(db);
    let ctx = Ctx::new(&runs);
    let mut active: Vec<&Run> = runs.values().filter(|r| r.finished_at.is_none()).collect();
    active.sort_by_key(|r| r.started_at.unwrap_or(0));
    let step = |r: &Run, n: &Node| TodayStep {
        run: r.id.clone(),
        run_name: r.name.clone(),
        step: n.id.clone(),
        title: n.title.clone(),
        from: (r.kind.as_deref() != Some("todo") && n.source != r.name).then(|| n.source.clone()),
        after: Vec::new(),
        after_other: Vec::new(),
    };
    Today {
        do_now: active
            .iter()
            .flat_map(|r| ready(r, Some(&ctx)).into_iter().map(|n| step(r, n)))
            .collect(),
        coming_up: active
            .iter()
            .flat_map(|r| {
                waiting(r, Some(&ctx)).into_iter().map(|w| TodayStep {
                    after: w.blocked_by.clone(),
                    after_other: w.blocked_ext.clone(),
                    ..step(r, &w.node)
                })
            })
            .collect(),
        running: active
            .iter()
            .map(|r| RunningRun {
                run: r.id.clone(),
                name: r.name.clone(),
                todo: r.kind.as_deref() == Some("todo"),
                flags: r
                    .flags
                    .iter()
                    .filter(|f| *f.1)
                    .map(|f| f.0.clone())
                    .collect(),
                done: r
                    .nodes
                    .iter()
                    .filter(|n| r.done.contains_key(&n.id))
                    .count(),
                total: r.nodes.len(),
                placed: r.attach.clone().filter(|a| runs.contains_key(&a.run)),
            })
            .collect(),
    }
}

/// To-dos not done and not deleted: starred first, then newest first.
pub fn open_backlog(db: &Db) -> Vec<&BacklogItem> {
    let mut items: Vec<&BacklogItem> = db
        .backlog
        .iter()
        .filter(|i| i.done_at.is_none() && i.removed.is_none())
        .collect();
    items.sort_by_key(|i| {
        (
            !i.starred.unwrap_or(false),
            std::cmp::Reverse(i.created_at.unwrap_or(0)),
        )
    });
    items
}

/// Something deleted or abandoned, which a `restore…` change brings back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeletedThing {
    pub kind: crate::change::Kind,
    pub id: String,
    /// Its name or title.
    pub name: String,
    /// Milliseconds since the epoch.
    pub at: i64,
    /// The device that removed it.
    pub by: String,
}

/// Deleted routines and to-dos and abandoned runs, newest first. A to-do's
/// run removed together with its to-do is listed once, as the to-do.
pub fn deleted(db: &Db) -> Vec<DeletedThing> {
    use crate::change::Kind;
    let thing = |kind, id: &str, name: &str, r: &Removed| DeletedThing {
        kind,
        id: id.to_string(),
        name: name.to_string(),
        at: r.at,
        by: r.by.clone(),
    };
    let todo_removed = |run: &Run| {
        run.backlog_id.as_ref().and_then(|t| {
            db.backlog
                .iter()
                .find(|i| &i.id == t)
                .and_then(|i| i.removed.as_ref())
        })
    };
    let mut out: Vec<DeletedThing> = db
        .routines
        .values()
        .filter_map(|r| Some(thing(Kind::Routine, &r.id, &r.name, r.removed.as_ref()?)))
        .chain(db.runs.values().filter_map(|r| {
            let gone = r.removed.as_ref()?;
            (todo_removed(r) != Some(gone)).then(|| thing(Kind::Run, &r.id, &r.name, gone))
        }))
        .chain(
            db.backlog
                .iter()
                .filter_map(|t| Some(thing(Kind::Todo, &t.id, &t.title, t.removed.as_ref()?))),
        )
        .collect();
    out.sort_by_key(|d| std::cmp::Reverse(d.at));
    out
}
