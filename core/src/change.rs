//! Changes: every way the stored data can be altered, as plain data.
//!
//! A [`Change`] says what the user (or Claude, through MCP) wants, never how
//! the data should look afterwards. Sync replays it: on the device that made
//! it, again on the server after other devices' changes, and on every device
//! after a pull (decision 0003, `docs/decisions/0003-sync.md`, arriving with
//! PR #2). That shapes the list:
//!
//! - **Absolute, not toggles.** "Mark this step done" and "star this to-do",
//!   never "toggle": the same change applied twice, or by two devices, gives
//!   the same result. Removing something already removed is a no-op.
//! - **One field per change** where the editor changes one field, so two
//!   devices editing different fields of a routine both keep their edit.
//! - **Nothing random or time-dependent inside [`apply`].** The time and who
//!   made the change come in a [`Stamp`]; new ids, when the change leaves
//!   them out, are made from the stamp.
//! - **Removing never erases.** Deleting a routine or to-do and abandoning a
//!   run set [`Removed`], and the matching `restore…` change takes it off.
//!   Removing a routine step returns an Undo that puts it back as it was.
//! - **A change that does not fit is refused, whole.** [`apply`] leaves the
//!   data as it was and says why in a [`Rejected`]; nothing is half done.

use crate::editor::{includes, would_cycle};
use crate::expand::expand;
use crate::runs::{Runs, has_cycle, is_finished};
use crate::types::*;
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

/// Who made a change, and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    /// The device (or server-side integration) that made it.
    pub client: String,
    /// That device's counter: every change it makes gets the next number.
    pub seq: u64,
    /// Milliseconds since the epoch, by the device's clock.
    pub at: i64,
}

impl Stamp {
    /// The id for something this change creates when the change names none:
    /// unique, because a device never reuses a number.
    fn new_id(&self) -> String {
        format!("{}-{}", self.client, self.seq)
    }

    fn removed(&self) -> Removed {
        Removed {
            at: self.at,
            by: self.client.clone(),
        }
    }
}

/// Where to put a run: before or after a step of another run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Placement {
    /// `after`: this run's first steps wait on that step. `before`: that step
    /// waits on this run's last steps.
    pub rel: Rel,
    /// The other run.
    pub run: String,
    /// The step in the other run.
    pub step: String,
}

impl Placement {
    fn attach(&self) -> Attach {
        Attach {
            rel: self.rel,
            run: self.run.clone(),
            id: self.step.clone(),
            extra: Extra::new(),
        }
    }

    fn of(a: &Attach) -> Self {
        Placement {
            rel: a.rel,
            run: a.run.clone(),
            step: a.id.clone(),
        }
    }
}

/// A step whose dependencies removing another step changed, and what they
/// were before.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rewired {
    pub step: String,
    pub after: Vec<Edge>,
}

/// Everything that can change the stored data. Serialized as an object whose
/// `op` names the change, the rest being its arguments, e.g.
/// `{"op": "markStep", "run": "r1", "step": "wake", "mark": "done"}`.
///
/// The doc comment on each variant is shown to Claude as the description of
/// the matching MCP tool, so it says what the change does to the user.
/// Changes that take something away carry `x-destructive`, which becomes
/// the tool's `destructiveHint`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Change {
    // ----- Routines -----
    /// Create a routine with no steps yet.
    CreateRoutine {
        /// Id for the new routine; made up when left out.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        routine: Option<String>,
        /// Its name; empty becomes "Untitled".
        name: String,
    },
    /// Rename a routine. Runs already started keep the old name.
    RenameRoutine {
        routine: String,
        /// Empty becomes "Untitled".
        name: String,
    },
    /// Delete a routine. Refused while another routine includes it. Runs
    /// already started from it are not affected. Can be restored.
    #[schemars(extend("x-destructive" = true))]
    DeleteRoutine { routine: String },
    /// Bring back a deleted routine.
    RestoreRoutine { routine: String },
    /// Add a task step to a routine.
    AddTask {
        routine: String,
        /// Id for the new step; made up when left out.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<String>,
        title: String,
        /// Steps of the same routine it comes after.
        #[serde(default)]
        after: Vec<String>,
        /// Only include it when this holds, e.g. `gym` or `!gym`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        when: Option<String>,
    },
    /// Add a step that is a whole other routine, spliced in when a run
    /// starts. Refused if that routine includes this one.
    AddSubRoutine {
        routine: String,
        /// Id for the new step; made up when left out.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<String>,
        /// The routine to include.
        includes: String,
        /// Steps of the same routine it comes after.
        #[serde(default)]
        after: Vec<String>,
        /// Only include it when this holds, e.g. `gym` or `!gym`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        when: Option<String>,
    },
    /// Change a task step's title.
    SetStepTitle {
        routine: String,
        step: String,
        title: String,
    },
    /// Change the condition a step is only included under (`gym`, `!gym`,
    /// `abroad`); empty means always.
    SetStepWhen {
        routine: String,
        step: String,
        when: String,
    },
    /// Change which routine a sub-routine step includes. Refused if that
    /// routine includes this one.
    SetStepRoutine {
        routine: String,
        step: String,
        includes: String,
    },
    /// Remove a step from a routine. What waited on it then waits on what it
    /// waited on. Its Undo puts it back as it was.
    #[schemars(extend("x-destructive" = true))]
    RemoveStep { routine: String, step: String },
    /// The Undo of `removeStep`: put a removed step back where it was, and
    /// give the steps that were rewired their old dependencies back. Made by
    /// the core, not offered as a tool.
    #[schemars(skip)]
    RestoreStep {
        routine: String,
        step: Box<Step>,
        /// Put it just before this step; at the end when left out or gone.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<String>,
        /// Steps whose dependencies removing it changed, with what they were.
        #[serde(default)]
        rewired: Vec<Rewired>,
    },
    /// Move a step in the routine's list (order only matters for display).
    MoveStep {
        routine: String,
        step: String,
        /// Put it just before this step; at the end when left out.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<String>,
    },
    /// Make a step come after another step of the same routine. Refused if
    /// that would make them wait on each other in a loop.
    AddDep {
        routine: String,
        step: String,
        /// The step it should come after.
        on: String,
        /// Only wait when this holds, e.g. `gym`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        when: Option<String>,
    },
    /// Stop a step coming after another step.
    #[schemars(extend("x-destructive" = true))]
    RemoveDep {
        routine: String,
        step: String,
        on: String,
    },

    // ----- Runs -----
    /// Start a run of a routine with the given options (flags). The run is a
    /// snapshot: editing the routine later does not change it.
    StartRun {
        /// Id for the new run; made up when left out.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<String>,
        routine: String,
        /// Option name to on/off; options left out are off.
        #[serde(default)]
        flags: Flags,
        /// Place it before or after a step of another run right away.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        place: Option<Placement>,
        /// The run's name and steps, filled in by [`apply`] when the run
        /// starts, so every device and the server keep the snapshot the
        /// starting device made, even if the routine was renamed, edited or
        /// deleted meanwhile.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(skip)]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(skip)]
        nodes: Option<Vec<Node>>,
    },
    /// Mark a step of a run done or skipped, or clear its mark. A run
    /// finishes when every step is marked, and reopens when one is cleared.
    MarkStep {
        run: String,
        step: String,
        /// `done`, `skipped`, or null to clear.
        mark: Option<Mark>,
    },
    /// Place a run before or after a step of another run, or unplace it.
    /// Refused if steps would end up waiting on each other in a loop.
    PlaceRun {
        run: String,
        /// Null to unplace.
        place: Option<Placement>,
    },
    /// Abandon a run. Runs placed relative to it no longer wait on it. Can be
    /// restored. For a to-do's run in Today, use `unpull_todo` (keeps the
    /// to-do) or `delete_todo` instead; abandoning it deletes the to-do too.
    #[schemars(extend("x-destructive" = true))]
    AbandonRun { run: String },
    /// Bring back an abandoned run.
    RestoreRun { run: String },

    // ----- Backlog -----
    /// Add a to-do to the backlog.
    AddTodo {
        /// Id for the new to-do; made up when left out.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        todo: Option<String>,
        title: String,
        #[serde(default)]
        starred: bool,
    },
    /// Change a to-do's title.
    RenameTodo { todo: String, title: String },
    /// Star or unstar a to-do.
    StarTodo { todo: String, starred: bool },
    /// Mark a to-do done, or not done. A to-do in Today finishes (or
    /// reopens) there too.
    CompleteTodo { todo: String, done: bool },
    /// Put a to-do into Today, optionally before or after a step of a run.
    PullTodo {
        todo: String,
        /// Id for the to-do's run in Today; made up when left out.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        place: Option<Placement>,
    },
    /// Take a to-do out of Today, back to the backlog.
    #[schemars(extend("x-destructive" = true))]
    UnpullTodo { todo: String },
    /// Delete a to-do (and take it out of Today). Can be restored.
    #[schemars(extend("x-destructive" = true))]
    DeleteTodo { todo: String },
    /// Bring back a deleted to-do.
    RestoreTodo { todo: String },

    // ----- Preferences -----
    /// Choose how Today lists steps.
    SetTodayMode { mode: TodayMode },
}

/// What a change refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Routine,
    Step,
    Run,
    Todo,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Kind::Routine => "routine",
            Kind::Step => "step",
            Kind::Run => "run",
            Kind::Todo => "to-do",
        })
    }
}

/// Why a change was refused. It is kept and shown to the user, never
/// dropped quietly; the messages are written for them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "reason",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Rejected {
    /// Nothing with that id exists.
    NotFound { kind: Kind, id: String },
    /// It was deleted or abandoned; restore it first.
    Removed { kind: Kind, id: String },
    /// Something with that id already exists.
    Exists { kind: Kind, id: String },
    /// The title is empty.
    EmptyTitle,
    /// The routine cannot be expanded into steps (a missing sub-routine, a
    /// routine that includes itself, a loop).
    CannotStart { message: String },
    /// No steps would be in the run.
    NoSteps { name: String, has_steps: bool },
    /// Placing would make steps across runs wait on each other in a loop.
    PlacementLoop,
    /// A run cannot be placed relative to its own steps.
    PlacedOnItself,
    /// The dependency would make steps wait on each other in a loop.
    StepLoop,
    /// The routine would include itself.
    IncludesItself { name: String },
    /// Other routines include it.
    UsedBy { names: Vec<String> },
    /// The step is a sub-routine, not a task (or the other way round).
    WrongStepType { step: String },
    /// The to-do is already in Today.
    AlreadyInToday,
    /// A run's steps, as sent, do not form a valid run.
    InvalidSnapshot,
}

impl fmt::Display for Rejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rejected::NotFound { kind, .. } => write!(f, "That {kind} no longer exists"),
            Rejected::Removed {
                kind: Kind::Run, ..
            } => write!(f, "That run was abandoned"),
            Rejected::Removed { kind, .. } => write!(f, "That {kind} was deleted"),
            Rejected::Exists { kind, id } => write!(f, "A {kind} with id \"{id}\" already exists"),
            Rejected::EmptyTitle => write!(f, "Give it a title"),
            Rejected::CannotStart { message } => f.write_str(message),
            Rejected::NoSteps { name, has_steps } if *has_steps => {
                write!(f, "{name} has no steps with these options")
            }
            Rejected::NoSteps { name, .. } => write!(f, "{name} has no steps yet"),
            Rejected::PlacementLoop => {
                write!(f, "That would make things wait on each other in a loop")
            }
            Rejected::PlacedOnItself => write!(f, "A run can't wait on its own steps"),
            Rejected::StepLoop => {
                write!(f, "That would make a loop — it already waits on this step")
            }
            Rejected::IncludesItself { name } => write!(f, "\"{name}\" would include itself"),
            Rejected::UsedBy { names } => write!(f, "Used by {}", names.join(", ")),
            Rejected::WrongStepType { .. } => write!(f, "That step can't be changed that way"),
            Rejected::AlreadyInToday => write!(f, "Already in Today"),
            Rejected::InvalidSnapshot => write!(f, "The run's steps don't fit together"),
        }
    }
}

impl std::error::Error for Rejected {}

/// What applying a change did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Applied {
    /// The change as applied, with made-up ids and a started run's steps
    /// filled in. This is what goes into the outbox and the log, so every
    /// replay creates the same things.
    pub change: Change,
    /// The id of what it created, if anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// The change that undoes it, offered as "Undo".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undo: Option<Change>,
    /// Runs this change finished ("Morning done ✓").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub finished: Vec<String>,
}

impl Applied {
    fn new(change: &Change) -> Self {
        Applied {
            change: change.clone(),
            created: None,
            undo: None,
            finished: Vec::new(),
        }
    }

    fn undo(mut self, undo: Change) -> Self {
        self.undo = Some(undo);
        self
    }
}

/// Apply `change` to `db`. On refusal `db` is left exactly as it was.
pub fn apply(db: &mut Db, stamp: &Stamp, change: &Change) -> Result<Applied, Rejected> {
    let mut next = db.clone();
    let applied = Apply {
        db: &mut next,
        stamp,
    }
    .change(change)?;
    *db = next;
    Ok(applied)
}

/// Routines that are not deleted, in stored order.
pub fn live_routines(db: &Db) -> IndexMap<String, Routine> {
    db.routines
        .iter()
        .filter(|(_, r)| r.removed.is_none())
        .map(|(k, r)| (k.clone(), r.clone()))
        .collect()
}

/// Runs that are not abandoned, in stored order. Placement and readiness are
/// worked out over these, so a run placed relative to an abandoned run no
/// longer waits on it.
pub fn live_runs(db: &Db) -> Runs {
    db.runs
        .iter()
        .filter(|(_, r)| r.removed.is_none())
        .map(|(k, r)| (k.clone(), r.clone()))
        .collect()
}

fn not_empty(title: &str) -> Result<String, Rejected> {
    let t = title.trim();
    if t.is_empty() {
        Err(Rejected::EmptyTitle)
    } else {
        Ok(t.to_string())
    }
}

fn name_or_untitled(name: &str) -> String {
    let t = name.trim();
    if t.is_empty() { "Untitled" } else { t }.to_string()
}

fn opt_when(when: &str) -> Option<String> {
    (!when.is_empty()).then(|| when.to_string())
}

struct Apply<'a> {
    db: &'a mut Db,
    stamp: &'a Stamp,
}

impl Apply<'_> {
    fn change(mut self, change: &Change) -> Result<Applied, Rejected> {
        use Change as C;
        let a = Applied::new(change);
        match change {
            C::CreateRoutine { routine, name } => {
                let id = routine.clone().unwrap_or_else(|| self.stamp.new_id());
                if self.db.routines.contains_key(&id) {
                    return Err(Rejected::Exists {
                        kind: Kind::Routine,
                        id,
                    });
                }
                self.db.routines.insert(
                    id.clone(),
                    Routine {
                        id: id.clone(),
                        name: name_or_untitled(name),
                        steps: Vec::new(),
                        last_flags: None,
                        removed: None,
                        extra: Extra::new(),
                    },
                );
                Ok(Applied {
                    change: C::CreateRoutine {
                        routine: Some(id.clone()),
                        name: name.clone(),
                    },
                    created: Some(id.clone()),
                    undo: Some(C::DeleteRoutine { routine: id }),
                    finished: Vec::new(),
                })
            }
            C::RenameRoutine { routine, name } => {
                let r = self.routine_mut(routine)?;
                let old = std::mem::replace(&mut r.name, name_or_untitled(name));
                Ok(a.undo(C::RenameRoutine {
                    routine: routine.clone(),
                    name: old,
                }))
            }
            C::DeleteRoutine { routine } => {
                let r = self
                    .db
                    .routines
                    .get(routine)
                    .ok_or_else(|| not_found(Kind::Routine, routine))?;
                if r.removed.is_some() {
                    return Ok(a);
                }
                let names: Vec<String> = self
                    .db
                    .routines
                    .values()
                    .filter(|o| o.removed.is_none() && &o.id != routine)
                    .filter(|o| sub_routines(o).any(|inc| inc == routine))
                    .map(|o| o.name.clone())
                    .collect();
                if !names.is_empty() {
                    return Err(Rejected::UsedBy { names });
                }
                let removed = self.stamp.removed();
                self.db.routines[routine.as_str()].removed = Some(removed);
                Ok(a.undo(C::RestoreRoutine {
                    routine: routine.clone(),
                }))
            }
            C::RestoreRoutine { routine } => {
                let r = self
                    .db
                    .routines
                    .get(routine)
                    .ok_or_else(|| not_found(Kind::Routine, routine))?;
                if r.removed.is_none() {
                    return Ok(a);
                }
                // What it includes must be there, and must not include it.
                let subs: Vec<String> = sub_routines(r).map(str::to_string).collect();
                for inc in &subs {
                    self.routine(inc)?;
                }
                self.db.routines[routine.as_str()].removed = None;
                if subs
                    .iter()
                    .any(|inc| includes(&self.db.routines, inc, routine))
                {
                    return Err(Rejected::IncludesItself {
                        name: self.db.routines[routine.as_str()].name.clone(),
                    });
                }
                Ok(a)
            }
            C::AddTask {
                routine,
                step,
                title,
                after,
                when,
            } => self.add_step(change, routine, step, after, when, |s| {
                s.title = Some(title.clone())
            }),
            C::AddSubRoutine {
                routine,
                step,
                includes: inc,
                after,
                when,
            } => {
                self.check_include(routine, inc)?;
                self.add_step(change, routine, step, after, when, |s| {
                    s.step_type = StepType::Routine;
                    s.routine_id = Some(inc.clone());
                })
            }
            C::SetStepTitle {
                routine,
                step,
                title,
            } => {
                let s = self.step_mut(routine, step)?;
                if s.step_type != StepType::Task {
                    return Err(Rejected::WrongStepType { step: step.clone() });
                }
                let old = s.title.replace(title.clone());
                Ok(a.undo(C::SetStepTitle {
                    routine: routine.clone(),
                    step: step.clone(),
                    title: old.unwrap_or_default(),
                }))
            }
            C::SetStepWhen {
                routine,
                step,
                when,
            } => {
                let s = self.step_mut(routine, step)?;
                let old = std::mem::replace(&mut s.when, opt_when(when));
                Ok(a.undo(C::SetStepWhen {
                    routine: routine.clone(),
                    step: step.clone(),
                    when: old.unwrap_or_default(),
                }))
            }
            C::SetStepRoutine {
                routine,
                step,
                includes: inc,
            } => {
                let i = self.step_index(routine, step)?;
                if self.db.routines[routine.as_str()].steps[i].step_type != StepType::Routine {
                    return Err(Rejected::WrongStepType { step: step.clone() });
                }
                self.check_include(routine, inc)?;
                let s = &mut self.db.routines[routine.as_str()].steps[i];
                let old = s.routine_id.replace(inc.clone());
                Ok(match old {
                    Some(old) => a.undo(C::SetStepRoutine {
                        routine: routine.clone(),
                        step: step.clone(),
                        includes: old,
                    }),
                    None => a,
                })
            }
            C::RemoveStep { routine, step } => {
                let gone = self.step_index(routine, step)?;
                let r = &mut self.db.routines[routine.as_str()];
                let before = r.steps.get(gone + 1).map(|s| s.id.clone());
                let gone_after = r.steps[gone].after.clone();
                let mut rewired = Vec::new();
                // Whatever waited on this step now waits on what it waited on.
                for i in 0..r.steps.len() {
                    if i == gone || !r.steps[i].after.iter().any(|e| &e.id == step) {
                        continue;
                    }
                    rewired.push(Rewired {
                        step: r.steps[i].id.clone(),
                        after: r.steps[i].after.clone(),
                    });
                    r.steps[i].after.retain(|e| &e.id != step);
                    for e in &gone_after {
                        let o = &r.steps[i];
                        if !o.after.iter().any(|x| x.id == e.id) && !would_cycle(r, &o.id, &e.id) {
                            r.steps[i].after.push(e.clone());
                        }
                    }
                }
                let removed = r.steps.remove(gone);
                Ok(a.undo(C::RestoreStep {
                    routine: routine.clone(),
                    step: Box::new(removed),
                    before,
                    rewired,
                }))
            }
            C::RestoreStep {
                routine,
                step,
                before,
                rewired,
            } => {
                let r = self.routine_mut(routine)?;
                if r.steps.iter().any(|s| s.id == step.id) {
                    return Err(Rejected::Exists {
                        kind: Kind::Step,
                        id: step.id.clone(),
                    });
                }
                let at = before
                    .as_ref()
                    .and_then(|b| r.steps.iter().position(|s| &s.id == b))
                    .unwrap_or(r.steps.len());
                r.steps.insert(at, (**step).clone());
                for w in rewired {
                    if let Some(s) = r.steps.iter_mut().rev().find(|s| s.id == w.step) {
                        s.after = w.after.clone();
                    }
                }
                Ok(a.undo(C::RemoveStep {
                    routine: routine.clone(),
                    step: step.id.clone(),
                }))
            }
            C::MoveStep {
                routine,
                step,
                before,
            } => {
                let i = self.step_index(routine, step)?;
                if let Some(b) = before {
                    if b == step {
                        return Ok(a);
                    }
                    self.step_index(routine, b)?;
                }
                let r = &mut self.db.routines[routine.as_str()];
                let was_before = r.steps.get(i + 1).map(|s| s.id.clone());
                let s = r.steps.remove(i);
                let j = before
                    .as_ref()
                    .and_then(|b| r.steps.iter().rposition(|s| &s.id == b))
                    .unwrap_or(r.steps.len());
                r.steps.insert(j, s);
                Ok(a.undo(C::MoveStep {
                    routine: routine.clone(),
                    step: step.clone(),
                    before: was_before,
                }))
            }
            C::AddDep {
                routine,
                step,
                on,
                when,
            } => {
                self.step_index(routine, on)?;
                let i = self.step_index(routine, step)?;
                let r = &mut self.db.routines[routine.as_str()];
                if r.steps[i].after.iter().any(|e| &e.id == on) {
                    return Ok(a);
                }
                if would_cycle(r, step, on) {
                    return Err(Rejected::StepLoop);
                }
                r.steps[i].after.push(Edge {
                    id: on.clone(),
                    when: when.clone().filter(|w| !w.is_empty()),
                    extra: Extra::new(),
                });
                Ok(a.undo(C::RemoveDep {
                    routine: routine.clone(),
                    step: step.clone(),
                    on: on.clone(),
                }))
            }
            C::RemoveDep { routine, step, on } => {
                let s = self.step_mut(routine, step)?;
                let Some(i) = s.after.iter().position(|e| &e.id == on) else {
                    return Ok(a);
                };
                let edge = s.after.remove(i);
                s.after.retain(|e| &e.id != on);
                Ok(a.undo(C::AddDep {
                    routine: routine.clone(),
                    step: step.clone(),
                    on: on.clone(),
                    when: edge.when,
                }))
            }

            C::StartRun {
                run,
                routine,
                flags,
                place,
                name,
                nodes,
            } => self.start_run(run, routine, flags, place, name, nodes),
            C::MarkStep { run, step, mark } => {
                let r = self.run_mut(run)?;
                if !r.nodes.iter().any(|n| &n.id == step) {
                    return Err(not_found(Kind::Step, step));
                }
                let old = match mark {
                    Some(m) => r.done.insert(step.clone(), *m),
                    None => r.done.shift_remove(step),
                };
                let mut a = a.undo(C::MarkStep {
                    run: run.clone(),
                    step: step.clone(),
                    mark: old,
                });
                a.finished = self.settle(run);
                Ok(a)
            }
            C::PlaceRun { run, place } => {
                let old = self.run_mut(run)?.attach.as_ref().map(Placement::of);
                self.place(run, place.as_ref())?;
                Ok(a.undo(C::PlaceRun {
                    run: run.clone(),
                    place: old,
                }))
            }
            C::AbandonRun { run } => {
                let removed = self.stamp.removed();
                let r = self
                    .db
                    .runs
                    .get_mut(run)
                    .ok_or_else(|| not_found(Kind::Run, run))?;
                if r.removed.is_some() {
                    return Ok(a);
                }
                r.removed = Some(removed.clone());
                if let Some(todo) = r.backlog_id.clone()
                    && let Some(t) = self.todo_entry(&todo)
                    && t.removed.is_none()
                {
                    t.removed = Some(removed);
                }
                Ok(a.undo(C::RestoreRun { run: run.clone() }))
            }
            C::RestoreRun { run } => {
                let r = self
                    .db
                    .runs
                    .get(run)
                    .ok_or_else(|| not_found(Kind::Run, run))?;
                let Some(gone) = r.removed.clone() else {
                    return Ok(a);
                };
                let todo = r.backlog_id.clone();
                if let Some(todo) = &todo
                    && let Some(t) = self.db.backlog.iter().find(|t| &t.id == todo)
                {
                    // Removed separately (deleted by someone else): leave it.
                    if t.removed.as_ref().is_some_and(|r| r != &gone) {
                        return Err(removed(Kind::Todo, todo));
                    }
                    // Already back in Today through another run.
                    if let Some(other) = &t.run_id
                        && other != run
                        && self.db.runs.get(other).is_some_and(|o| o.removed.is_none())
                    {
                        return Err(Rejected::AlreadyInToday);
                    }
                }
                self.db.runs[run.as_str()].removed = None;
                if let Some(todo) = &todo
                    && let Some(t) = self.todo_entry(todo)
                {
                    t.removed = None;
                    t.run_id = Some(run.clone());
                }
                self.check_no_loop()?;
                Ok(a)
            }

            C::AddTodo {
                todo,
                title,
                starred,
            } => {
                let title = not_empty(title)?;
                let id = todo.clone().unwrap_or_else(|| self.stamp.new_id());
                if self.todo_entry(&id).is_some() {
                    return Err(Rejected::Exists {
                        kind: Kind::Todo,
                        id,
                    });
                }
                self.db.backlog.push(BacklogItem {
                    id: id.clone(),
                    title: title.clone(),
                    starred: Some(*starred),
                    created_at: Some(self.stamp.at),
                    done_at: None,
                    run_id: None,
                    removed: None,
                    extra: Extra::new(),
                });
                Ok(Applied {
                    change: C::AddTodo {
                        todo: Some(id.clone()),
                        title,
                        starred: *starred,
                    },
                    created: Some(id.clone()),
                    undo: Some(C::DeleteTodo { todo: id }),
                    finished: Vec::new(),
                })
            }
            C::RenameTodo { todo, title } => {
                let title = not_empty(title)?;
                let t = self.todo_mut(todo)?;
                let old = std::mem::replace(&mut t.title, title);
                Ok(a.undo(C::RenameTodo {
                    todo: todo.clone(),
                    title: old,
                }))
            }
            C::StarTodo { todo, starred } => {
                let t = self.todo_mut(todo)?;
                let old = t.starred.replace(*starred).unwrap_or(false);
                Ok(a.undo(C::StarTodo {
                    todo: todo.clone(),
                    starred: old,
                }))
            }
            C::CompleteTodo { todo, done } => {
                let at = self.stamp.at;
                let t = self.todo_mut(todo)?;
                let was = t.done_at.is_some();
                t.done_at = done.then_some(at);
                let run = t.run_id.clone();
                let mut a = a.undo(C::CompleteTodo {
                    todo: todo.clone(),
                    done: was,
                });
                // A to-do in Today is its run: finishing one finishes the other.
                if let Some(run) = run
                    && let Ok(r) = self.run_mut(&run)
                {
                    if *done {
                        for n in &r.nodes {
                            r.done.entry(n.id.clone()).or_insert(Mark::Done);
                        }
                    } else {
                        r.done.clear();
                    }
                    a.finished = self.settle(&run);
                }
                Ok(a)
            }
            C::PullTodo { todo, run, place } => {
                let t = self.todo_mut(todo)?;
                let in_today = t.run_id.clone();
                let title = t.title.clone();
                if let Some(r) = in_today
                    && self.run_mut(&r).is_ok()
                {
                    return Err(Rejected::AlreadyInToday);
                }
                let id = run.clone().unwrap_or_else(|| self.stamp.new_id());
                if self.db.runs.contains_key(&id) {
                    return Err(Rejected::Exists {
                        kind: Kind::Run,
                        id,
                    });
                }
                self.db.runs.insert(
                    id.clone(),
                    Run {
                        id: id.clone(),
                        routine_id: None,
                        kind: Some("todo".into()),
                        backlog_id: Some(todo.clone()),
                        name: title.clone(),
                        flags: Flags::new(),
                        nodes: vec![Node {
                            id: "task".into(),
                            title: title.clone(),
                            deps: Vec::new(),
                            source: title,
                            extra: Extra::new(),
                        }],
                        done: IndexMap::new(),
                        started_at: Some(self.stamp.at),
                        finished_at: None,
                        attach: None,
                        removed: None,
                        extra: Extra::new(),
                    },
                );
                self.place(&id, place.as_ref())?;
                self.todo_mut(todo)?.run_id = Some(id.clone());
                Ok(Applied {
                    change: C::PullTodo {
                        todo: todo.clone(),
                        run: Some(id.clone()),
                        place: place.clone(),
                    },
                    created: Some(id),
                    undo: Some(C::UnpullTodo { todo: todo.clone() }),
                    finished: Vec::new(),
                })
            }
            C::UnpullTodo { todo } => {
                let removed = self.stamp.removed();
                let t = self
                    .todo_entry(todo)
                    .ok_or_else(|| not_found(Kind::Todo, todo))?;
                // Not in Today (any more): nothing to do.
                let Some(run) = t.run_id.clone().filter(|r| self.run_mut(r).is_ok()) else {
                    return Ok(a);
                };
                self.run_mut(&run)?.removed = Some(removed);
                self.todo_mut(todo)?.run_id = None;
                Ok(a.undo(C::RestoreRun { run }))
            }
            C::DeleteTodo { todo } => {
                let removed = self.stamp.removed();
                let t = self
                    .todo_entry(todo)
                    .ok_or_else(|| not_found(Kind::Todo, todo))?;
                if t.removed.is_some() {
                    return Ok(a);
                }
                t.removed = Some(removed.clone());
                if let Some(run) = t.run_id.clone()
                    && let Ok(r) = self.run_mut(&run)
                {
                    r.removed = Some(removed);
                }
                Ok(a.undo(C::RestoreTodo { todo: todo.clone() }))
            }
            C::RestoreTodo { todo } => {
                let t = self
                    .todo_entry(todo)
                    .ok_or_else(|| not_found(Kind::Todo, todo))?;
                let Some(gone) = t.removed.take() else {
                    return Ok(a);
                };
                // Its run comes back only if it went with the to-do.
                if let Some(run) = t.run_id.clone()
                    && let Some(r) = self.db.runs.get_mut(&run)
                    && r.removed.as_ref() == Some(&gone)
                {
                    r.removed = None;
                }
                self.check_no_loop()?;
                Ok(a)
            }

            C::SetTodayMode { mode } => {
                let prefs = self.db.prefs.get_or_insert_with(|| Prefs {
                    today_mode: None,
                    extra: Extra::new(),
                });
                let old = prefs.today_mode.replace(*mode).unwrap_or(TodayMode::Mixed);
                Ok(a.undo(C::SetTodayMode { mode: old }))
            }
        }
    }

    // ----- lookups that refuse missing and removed things -----

    fn routine(&self, id: &str) -> Result<&Routine, Rejected> {
        match self.db.routines.get(id) {
            None => Err(not_found(Kind::Routine, id)),
            Some(r) if r.removed.is_some() => Err(removed(Kind::Routine, id)),
            Some(r) => Ok(r),
        }
    }

    fn routine_mut(&mut self, id: &str) -> Result<&mut Routine, Rejected> {
        self.routine(id)?;
        Ok(&mut self.db.routines[id])
    }

    /// Where the step is in the routine: the last step with that id, as
    /// edges name it.
    fn step_index(&self, routine: &str, step: &str) -> Result<usize, Rejected> {
        self.routine(routine)?
            .steps
            .iter()
            .rposition(|s| s.id == step)
            .ok_or_else(|| not_found(Kind::Step, step))
    }

    fn step_mut(&mut self, routine: &str, step: &str) -> Result<&mut Step, Rejected> {
        let i = self.step_index(routine, step)?;
        Ok(&mut self.db.routines[routine].steps[i])
    }

    fn run_mut(&mut self, id: &str) -> Result<&mut Run, Rejected> {
        match self.db.runs.get_mut(id) {
            None => Err(not_found(Kind::Run, id)),
            Some(r) if r.removed.is_some() => Err(removed(Kind::Run, id)),
            Some(r) => Ok(r),
        }
    }

    /// The to-do, removed or not.
    fn todo_entry(&mut self, id: &str) -> Option<&mut BacklogItem> {
        self.db.backlog.iter_mut().find(|t| t.id == id)
    }

    fn todo_mut(&mut self, id: &str) -> Result<&mut BacklogItem, Rejected> {
        match self.todo_entry(id) {
            None => Err(not_found(Kind::Todo, id)),
            Some(t) if t.removed.is_some() => Err(removed(Kind::Todo, id)),
            Some(t) => Ok(t),
        }
    }

    // ----- rules shared by several changes -----

    /// Can `routine` include `inc` as a sub-routine? Follows deleted
    /// routines too, so restoring one later cannot close a loop.
    fn check_include(&self, routine: &str, inc: &str) -> Result<(), Rejected> {
        self.routine(routine)?;
        self.routine(inc)?;
        if includes(&self.db.routines, inc, routine) {
            return Err(Rejected::IncludesItself {
                name: self.db.routines[routine].name.clone(),
            });
        }
        Ok(())
    }

    /// Add a step to a routine for `AddTask`/`AddSubRoutine` (`change`):
    /// `fill` sets what differs between the two.
    fn add_step(
        &mut self,
        change: &Change,
        routine: &str,
        step: &Option<String>,
        after: &[String],
        when: &Option<String>,
        fill: impl FnOnce(&mut Step),
    ) -> Result<Applied, Rejected> {
        let id = step.clone().unwrap_or_else(|| self.stamp.new_id());
        let r = self.routine_mut(routine)?;
        if r.steps.iter().any(|s| s.id == id) {
            return Err(Rejected::Exists {
                kind: Kind::Step,
                id,
            });
        }
        if let Some(missing) = after.iter().find(|a| !r.steps.iter().any(|s| &s.id == *a)) {
            return Err(not_found(Kind::Step, missing));
        }
        let mut seen = HashSet::new();
        let mut new = Step {
            id: id.clone(),
            step_type: StepType::Task,
            title: None,
            routine_id: None,
            when: when.clone().filter(|w| !w.is_empty()),
            after: after
                .iter()
                .filter(|a| seen.insert(a.as_str()))
                .map(|a| Edge {
                    id: a.clone(),
                    when: None,
                    extra: Extra::new(),
                })
                .collect(),
            extra: Extra::new(),
        };
        fill(&mut new);
        r.steps.push(new);
        let mut change = change.clone();
        if let Change::AddTask { step, .. } | Change::AddSubRoutine { step, .. } = &mut change {
            *step = Some(id.clone());
        }
        Ok(Applied {
            change,
            created: Some(id.clone()),
            undo: Some(Change::RemoveStep {
                routine: routine.to_string(),
                step: id,
            }),
            finished: Vec::new(),
        })
    }

    fn start_run(
        &mut self,
        run: &Option<String>,
        routine: &str,
        flags: &Flags,
        place: &Option<Placement>,
        name: &Option<String>,
        nodes: &Option<Vec<Node>>,
    ) -> Result<Applied, Rejected> {
        let (name, nodes) = match nodes {
            // A run already started on a device: keep its snapshot, whatever
            // happened to the routine since.
            Some(n) => {
                check_snapshot(n)?;
                let name = name
                    .clone()
                    .or_else(|| self.db.routines.get(routine).map(|r| r.name.clone()))
                    .ok_or(Rejected::InvalidSnapshot)?;
                (name, n.clone())
            }
            None => {
                let r = self.routine(routine)?;
                let n = expand(&live_routines(self.db), routine, flags).map_err(|e| {
                    Rejected::CannotStart {
                        message: e.to_string(),
                    }
                })?;
                if n.is_empty() {
                    return Err(Rejected::NoSteps {
                        name: r.name.clone(),
                        has_steps: !r.steps.is_empty(),
                    });
                }
                (r.name.clone(), n)
            }
        };
        let id = run.clone().unwrap_or_else(|| self.stamp.new_id());
        if self.db.runs.contains_key(&id) {
            return Err(Rejected::Exists {
                kind: Kind::Run,
                id,
            });
        }
        if let Ok(r) = self.routine_mut(routine) {
            r.last_flags = Some(flags.clone());
        }
        self.db.runs.insert(
            id.clone(),
            Run {
                id: id.clone(),
                routine_id: Some(routine.to_string()),
                kind: None,
                backlog_id: None,
                name: name.clone(),
                flags: flags.clone(),
                nodes: nodes.clone(),
                done: IndexMap::new(),
                started_at: Some(self.stamp.at),
                finished_at: None,
                attach: None,
                removed: None,
                extra: Extra::new(),
            },
        );
        self.place(&id, place.as_ref())?;
        Ok(Applied {
            change: Change::StartRun {
                run: Some(id.clone()),
                routine: routine.to_string(),
                flags: flags.clone(),
                place: place.clone(),
                name: Some(name),
                nodes: Some(nodes),
            },
            created: Some(id.clone()),
            undo: Some(Change::AbandonRun { run: id }),
            finished: Vec::new(),
        })
    }

    /// Place run `id` (or unplace it), refusing loops.
    fn place(&mut self, id: &str, place: Option<&Placement>) -> Result<(), Rejected> {
        if let Some(p) = place {
            if p.run == id {
                return Err(Rejected::PlacedOnItself);
            }
            let other = self.run_mut(&p.run)?;
            if !other.nodes.iter().any(|n| n.id == p.step) {
                return Err(not_found(Kind::Step, &p.step));
            }
        }
        self.run_mut(id)?.attach = place.map(Placement::attach);
        self.check_no_loop()
    }

    /// Placements across the runs that are not abandoned must not make steps
    /// wait on each other in a loop.
    fn check_no_loop(&self) -> Result<(), Rejected> {
        if has_cycle(&live_runs(self.db)) {
            return Err(Rejected::PlacementLoop);
        }
        Ok(())
    }

    /// A run finishes by itself when its last step is resolved, and reopens
    /// if one is cleared. A to-do run's to-do follows it. Returns the run if
    /// it just finished.
    fn settle(&mut self, id: &str) -> Vec<String> {
        let at = self.stamp.at;
        let r = &mut self.db.runs[id];
        let mut finished = Vec::new();
        if is_finished(r) {
            if r.finished_at.is_none() {
                r.finished_at = Some(at);
                finished.push(id.to_string());
            }
        } else {
            r.finished_at = None;
        }
        let (todo, fin) = (r.backlog_id.clone(), r.finished_at);
        if let Some(todo) = todo
            && let Some(t) = self.todo_entry(&todo)
        {
            t.done_at = fin;
        }
        finished
    }
}

/// The routines a routine includes as sub-routine steps.
fn sub_routines(r: &Routine) -> impl Iterator<Item = &str> {
    r.steps
        .iter()
        .filter(|s| s.step_type == StepType::Routine)
        .filter_map(|s| s.routine_id.as_deref())
}

/// A run's steps as a device sent them: at least one, ids unique, and each
/// waiting only on steps listed before it (so there is no loop).
fn check_snapshot(nodes: &[Node]) -> Result<(), Rejected> {
    let mut seen = HashSet::new();
    for n in nodes {
        if !n.deps.iter().all(|d| seen.contains(d.as_str())) || !seen.insert(n.id.as_str()) {
            return Err(Rejected::InvalidSnapshot);
        }
    }
    if nodes.is_empty() {
        return Err(Rejected::InvalidSnapshot);
    }
    Ok(())
}

fn not_found(kind: Kind, id: &str) -> Rejected {
    Rejected::NotFound {
        kind,
        id: id.to_string(),
    }
}

fn removed(kind: Kind, id: &str) -> Rejected {
    Rejected::Removed {
        kind,
        id: id.to_string(),
    }
}
