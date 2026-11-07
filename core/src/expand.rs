//! Flattening a routine, with its sub-routines, into the steps of a run.

use crate::cond::cond_holds;
use crate::types::{Extra, Flags, Node, Routine, Step, StepType};
use indexmap::{IndexMap, IndexSet};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Why a routine cannot be started. The messages are shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpandError {
    /// The routine, or a routine it includes, does not exist.
    MissingRoutine,
    /// The named routine includes itself, directly or through others.
    IncludesItself(String),
    /// Steps of the named routine wait on each other in a loop.
    Circular(String),
}

impl fmt::Display for ExpandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExpandError::MissingRoutine => write!(f, "A sub-routine no longer exists"),
            ExpandError::IncludesItself(name) => write!(f, "\"{name}\" includes itself"),
            ExpandError::Circular(name) => write!(f, "Circular dependency in \"{name}\""),
        }
    }
}

impl std::error::Error for ExpandError {}

/// Flatten routine `root_id` (with sub-routines) into concrete steps for the
/// given flags.
///
/// Steps whose condition fails are removed and their dependencies bridged:
/// anything that waited on a removed step waits on what the removed step
/// waited on. A sub-routine's first steps wait on what the including step
/// waited on, and what waited on the including step waits on the
/// sub-routine's last steps. Steps come out in an order where every step
/// follows the steps it waits on.
pub fn expand(
    routines: &IndexMap<String, Routine>,
    root_id: &str,
    flags: &Flags,
) -> Result<Vec<Node>, ExpandError> {
    let mut ex = Expander {
        routines,
        flags,
        nodes: Vec::new(),
        stack: Vec::new(),
    };
    ex.walk(Some(root_id), String::new(), Vec::new())?;
    Ok(ex.nodes)
}

struct Expander<'a> {
    routines: &'a IndexMap<String, Routine>,
    flags: &'a Flags,
    nodes: Vec<Node>,
    /// The routines being walked, outermost first.
    stack: Vec<&'a str>,
}

/// One routine being walked.
struct Scope<'a> {
    routine: &'a Routine,
    /// Prefix for the ids of this routine's steps.
    prefix: String,
    /// What this routine's first steps wait on.
    entry_deps: Vec<String>,
    /// A step id names the last step with that id.
    by_id: HashMap<&'a str, &'a Step>,
    /// Step id to the node ids anything after that step must wait on.
    memo: HashMap<&'a str, Vec<String>>,
    visiting: HashSet<&'a str>,
}

impl<'a> Expander<'a> {
    /// Add the nodes of a routine; returns the ids of its last steps, or
    /// `entry_deps` if it added none.
    fn walk(
        &mut self,
        routine_id: Option<&'a str>,
        prefix: String,
        entry_deps: Vec<String>,
    ) -> Result<Vec<String>, ExpandError> {
        let (id, routine) = routine_id
            .and_then(|id| self.routines.get_key_value(id))
            .ok_or(ExpandError::MissingRoutine)?;
        if self.stack.contains(&id.as_str()) {
            return Err(ExpandError::IncludesItself(routine.name.clone()));
        }
        self.stack.push(id);
        let mut scope = Scope {
            routine,
            prefix,
            entry_deps,
            by_id: routine.steps.iter().map(|s| (s.id.as_str(), s)).collect(),
            memo: HashMap::new(),
            visiting: HashSet::new(),
        };
        let start = self.nodes.len();
        for step in &routine.steps {
            self.exit_of(&mut scope, step)?;
        }
        self.stack.pop();

        let mine = &self.nodes[start..];
        if mine.is_empty() {
            return Ok(scope.entry_deps);
        }
        let consumed: HashSet<&str> = mine
            .iter()
            .flat_map(|n| n.deps.iter().map(String::as_str))
            .collect();
        Ok(mine
            .iter()
            .filter(|n| !consumed.contains(n.id.as_str()))
            .map(|n| n.id.clone())
            .collect())
    }

    /// The node ids anything that waits on `step` must wait on.
    fn exit_of(
        &mut self,
        scope: &mut Scope<'a>,
        step: &'a Step,
    ) -> Result<Vec<String>, ExpandError> {
        if let Some(out) = scope.memo.get(step.id.as_str()) {
            return Ok(out.clone());
        }
        if !scope.visiting.insert(step.id.as_str()) {
            return Err(ExpandError::Circular(scope.routine.name.clone()));
        }

        let mut deps = IndexSet::new();
        let mut any_edge = false;
        for e in &step.after {
            let Some(&prev) = scope.by_id.get(e.id.as_str()) else {
                continue;
            };
            if !cond_holds(e.when.as_deref(), self.flags) {
                continue;
            }
            any_edge = true;
            deps.extend(self.exit_of(scope, prev)?);
        }
        // With no edge left (none, or all conditional and off), the step
        // waits on what the routine's first steps wait on.
        let deps: Vec<String> = if any_edge {
            deps.into_iter().collect()
        } else {
            scope.entry_deps.clone()
        };

        let out = if !cond_holds(step.when.as_deref(), self.flags) {
            deps
        } else if step.step_type == StepType::Routine {
            let prefix = format!("{}{}/", scope.prefix, step.id);
            self.walk(step.routine_id.as_deref(), prefix, deps)?
        } else {
            let id = format!("{}{}", scope.prefix, step.id);
            self.nodes.push(Node {
                id: id.clone(),
                title: match step.title.as_deref() {
                    Some(t) if !t.is_empty() => t.to_string(),
                    _ => "(untitled)".to_string(),
                },
                deps,
                source: scope.routine.name.clone(),
                extra: Extra::new(),
            });
            vec![id]
        };
        scope.visiting.remove(step.id.as_str());
        scope.memo.insert(step.id.as_str(), out.clone());
        Ok(out)
    }
}
