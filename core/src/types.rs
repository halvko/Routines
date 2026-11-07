//! The stored data, as the web app saves it under `localStorage` key
//! `routines.v1`. Field names follow the JSON (camelCase).
//!
//! Every struct keeps fields it does not know in `extra`, so data written by
//! a newer version survives a round trip through this one. Maps keep the
//! order they were read in, as JavaScript objects do: some rules (which run
//! is placed first, the order of blockers) follow that order.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Fields a struct does not model, kept as they were read.
pub type Extra = IndexMap<String, Value>;

/// Flag name to on/off, chosen when a run starts. Names are lowercase as
/// [`crate::collect_flags`] returns them; conditions are matched against the
/// keys exactly.
pub type Flags = IndexMap<String, bool>;

/// Everything the app stores.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Db {
    pub routines: IndexMap<String, Routine>,
    pub runs: IndexMap<String, Run>,
    /// Missing in data saved before the backlog existed.
    #[serde(default)]
    pub backlog: Vec<BacklogItem>,
    /// Missing in data saved before preferences existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefs: Option<Prefs>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A routine template: steps that wait on each other.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Routine {
    pub id: String,
    pub name: String,
    pub steps: Vec<Step>,
    /// The flags chosen the last time a run of it started, offered next time.
    #[serde(rename = "lastFlags", default, skip_serializing_if = "Option::is_none")]
    pub last_flags: Option<Flags>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepType {
    Task,
    /// The step is a whole other routine, spliced in when a run starts.
    Routine,
}

/// One step of a routine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    #[serde(rename = "type")]
    pub step_type: StepType,
    /// A task's title; empty or missing shows as "(untitled)".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The included routine, for a step of type `routine`.
    #[serde(rename = "routineId", default, skip_serializing_if = "Option::is_none")]
    pub routine_id: Option<String>,
    /// Condition on flags; see [`crate::parse_cond`]. Empty means always.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    /// The steps this one comes after.
    #[serde(default)]
    pub after: Vec<Edge>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// "Comes after step `id`", optionally only when a condition holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A started routine: a snapshot of its steps, flattened for its flags.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    /// The routine it was started from; missing for a backlog to-do.
    #[serde(rename = "routineId", default, skip_serializing_if = "Option::is_none")]
    pub routine_id: Option<String>,
    /// `"todo"` for a backlog item pulled into Today.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The backlog item a to-do run came from.
    #[serde(rename = "backlogId", default, skip_serializing_if = "Option::is_none")]
    pub backlog_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub flags: Flags,
    pub nodes: Vec<Node>,
    /// Step id to how it was resolved; unresolved steps are absent.
    #[serde(default)]
    pub done: IndexMap<String, Mark>,
    /// Milliseconds since the epoch, as `Date.now()`.
    #[serde(rename = "startedAt", default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(
        rename = "finishedAt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub finished_at: Option<i64>,
    /// Where the run is placed relative to a step of another run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attach: Option<Attach>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A concrete step of a run. Ids of steps from sub-routines are prefixed with
/// the including steps' ids: `bf/coffee`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub title: String,
    /// Ids of steps in the same run this one waits on.
    pub deps: Vec<String>,
    /// Name of the routine the step came from.
    pub source: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mark {
    Done,
    /// Counts as resolved, so what waited on it can go ahead.
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rel {
    /// The run's first steps wait on the other run's step.
    After,
    /// The other run's step waits on this run's last steps.
    Before,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attach {
    pub rel: Rel,
    /// The other run.
    pub run: String,
    /// The step in the other run.
    pub id: String,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A step in some run.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeRef {
    pub run: String,
    pub id: String,
}

/// A plain to-do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BacklogItem {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starred: Option<bool>,
    #[serde(rename = "createdAt", default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(rename = "doneAt", default, skip_serializing_if = "Option::is_none")]
    pub done_at: Option<i64>,
    /// The to-do run it was pulled into, while it is in Today.
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Prefs {
    /// `"mixed"` or `"grouped"`.
    #[serde(rename = "todayMode", default, skip_serializing_if = "Option::is_none")]
    pub today_mode: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}
