//! Guards the routine editor uses before adding an edge or a sub-routine.

use crate::types::{Routine, StepType};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

/// Would making `step_id` wait on `dep_id` create a loop inside one routine?
pub fn would_cycle(routine: &Routine, step_id: &str, dep_id: &str) -> bool {
    if step_id == dep_id {
        return true;
    }
    // A step id names the last step with that id.
    let by_id: HashMap<&str, _> = routine.steps.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut seen = HashSet::new();
    // Does `dep_id` already (transitively) wait on `step_id`?
    let mut stack = vec![dep_id];
    while let Some(from) = stack.pop() {
        if from == step_id {
            return true;
        }
        if !seen.insert(from) {
            continue;
        }
        if let Some(s) = by_id.get(from) {
            // Reversed so edges are followed in the order they are listed.
            stack.extend(s.after.iter().rev().map(|e| e.id.as_str()));
        }
    }
    false
}

/// Does routine `rid` include routine `target`, directly or through others?
/// A routine counts as including itself.
pub fn includes(routines: &IndexMap<String, Routine>, rid: &str, target: &str) -> bool {
    let mut seen = HashSet::new();
    let mut stack = vec![rid];
    while let Some(id) = stack.pop() {
        if id == target {
            return true;
        }
        let Some(r) = routines.get(id) else { continue };
        if !seen.insert(id) {
            continue;
        }
        stack.extend(
            r.steps
                .iter()
                .rev()
                .filter(|s| s.step_type == StepType::Routine)
                .filter_map(|s| s.routine_id.as_deref()),
        );
    }
    false
}
