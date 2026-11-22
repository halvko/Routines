//! Sync between devices (decision 0003), as plain state machines with no I/O.
//!
//! - A device keeps a [`Client`]: the state as of the last change it pulled
//!   from the server (the *base*), plus an *outbox* of its own changes the
//!   server has not taken yet. What it shows is the base with the outbox
//!   replayed on top.
//! - The server keeps one [`Log`] per user. It applies each pushed change
//!   with the same [`apply`], numbers it, and the shell stores the
//!   resulting [`LogEntry`] and tells the user's other devices.
//! - Devices pull the entries after the last number they have, take them as
//!   their new base, and replay what is still in their outbox on top.
//!
//! Changes are idempotent by `(client, seq)`, so pushing the same outbox
//! twice is safe. A change the server cannot apply is still logged, with
//! the reason, and every device shows it as a notice: nothing is dropped
//! quietly.
//!
//! The MCP server is one more client: it makes changes with its own client
//! id, applies them to the user's [`Log`] like any push, and other devices
//! pick them up the same way.

use crate::change::{Applied, Change, Rejected, Stamp, apply};
use crate::seed::seed;
use crate::types::Db;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// A change as it travels: who made it, their counter, and when.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub client: String,
    pub seq: u64,
    pub at: i64,
    pub change: Change,
}

impl Envelope {
    pub fn stamp(&self) -> Stamp {
        Stamp {
            client: self.client.clone(),
            seq: self.seq,
            at: self.at,
        }
    }
}

/// A change as the server logged it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    /// Its number in the user's log: 1, 2, 3, … with no gaps.
    pub n: u64,
    #[serde(flatten)]
    pub envelope: Envelope,
    /// Why the server could not apply it; it then changed nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejected: Option<Rejected>,
}

/// The server's side: the user's current state and how far each device's
/// changes have been taken.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Log {
    pub db: Db,
    /// The number of the last entry.
    pub n: u64,
    /// The last `seq` taken from each client.
    pub seen: IndexMap<String, u64>,
}

impl Default for Log {
    fn default() -> Self {
        Log {
            db: seed(),
            n: 0,
            seen: IndexMap::new(),
        }
    }
}

impl Log {
    /// Take one pushed change. Returns the entry to store and announce, or
    /// `None` if this change was taken before (a retry).
    pub fn push(&mut self, envelope: Envelope) -> Option<LogEntry> {
        let seen = self.seen.entry(envelope.client.clone()).or_insert(0);
        if envelope.seq <= *seen {
            return None;
        }
        *seen = envelope.seq;
        self.n += 1;
        let (envelope, rejected) = match apply(&mut self.db, &envelope.stamp(), &envelope.change) {
            Ok(a) => (
                Envelope {
                    change: a.change,
                    ..envelope
                },
                None,
            ),
            Err(r) => (envelope, Some(r)),
        };
        Some(LogEntry {
            n: self.n,
            envelope,
            rejected,
        })
    }
}

/// Something that happened, for [`Client::update`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Event {
    /// The user (on this device) wants this change, at this time.
    Change { at: i64, change: Change },
    /// The shell (re)connected to the server.
    Connected,
    /// Log entries arrived from the server, in order.
    Pulled { entries: Vec<LogEntry> },
    /// The user has seen notice `n` (a log entry number).
    Dismiss { n: u64 },
}

/// What the shell should do after [`Client::update`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Effect {
    /// Store [`Client::saved`]; the device's data changed.
    Save,
    /// Send these changes to the server (all of the outbox; resending is
    /// safe).
    Push { changes: Vec<Envelope> },
    /// Ask the server for the entries after number `after`.
    Pull { after: u64 },
    /// A change the user just made was applied: show its Undo, and any run
    /// it finished.
    Applied(Applied),
    /// A change the user just made was refused; show why. Nothing changed.
    Refused(Rejected),
    /// A change could not be applied on the server (one made on another
    /// device, or one made here earlier that no longer fits). Show it as a
    /// notice the user opens when they choose; it never blocks anything.
    Notice(LogEntry),
}

/// A device's side of sync.
#[derive(Debug, Clone, PartialEq)]
pub struct Client {
    saved: Saved,
    /// The base with the outbox replayed on top: what the device shows.
    state: Db,
}

/// What a device stores, so it can carry on after being closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    /// This device's id; unique per device and never reused.
    pub client: String,
    /// The `seq` of this device's last change.
    pub seq: u64,
    /// The state as of log entry `base_n`.
    pub base: Db,
    pub base_n: u64,
    /// This device's changes not yet in the log, oldest first.
    pub outbox: Vec<Envelope>,
    /// Log entries the server could not apply, until the user dismisses
    /// them.
    pub notices: Vec<LogEntry>,
}

impl Client {
    /// A device that has never synced, starting from the seed routines (as
    /// the server's [`Log`] does, so the first pull fits on top).
    pub fn new(client: impl Into<String>) -> Self {
        Self::restore(Saved {
            client: client.into(),
            seq: 0,
            base: seed(),
            base_n: 0,
            outbox: Vec::new(),
            notices: Vec::new(),
        })
    }

    /// Carry on from what [`Effect::Save`] stored.
    pub fn restore(saved: Saved) -> Self {
        let mut c = Client {
            state: saved.base.clone(),
            saved,
        };
        c.replay();
        c
    }

    /// What to show.
    pub fn state(&self) -> &Db {
        &self.state
    }

    /// What to store.
    pub fn saved(&self) -> &Saved {
        &self.saved
    }

    pub fn notices(&self) -> &[LogEntry] {
        &self.saved.notices
    }

    /// Handle one event. Never waits and never does I/O; the shell carries
    /// out the effects and reports results as new events (decision 0002).
    pub fn update(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::Change { at, change } => self.change(at, change),
            Event::Connected => vec![self.push(), self.pull()]
                .into_iter()
                .flatten()
                .collect(),
            Event::Pulled { entries } => self.pulled(entries),
            Event::Dismiss { n } => {
                let before = self.saved.notices.len();
                self.saved.notices.retain(|e| e.n != n);
                if self.saved.notices.len() == before {
                    vec![]
                } else {
                    vec![Effect::Save]
                }
            }
        }
    }

    fn change(&mut self, at: i64, change: Change) -> Vec<Effect> {
        let stamp = Stamp {
            client: self.saved.client.clone(),
            seq: self.saved.seq + 1,
            at,
        };
        match apply(&mut self.state, &stamp, &change) {
            Err(r) => vec![Effect::Refused(r)],
            Ok(applied) => {
                self.saved.seq = stamp.seq;
                self.saved.outbox.push(Envelope {
                    client: stamp.client,
                    seq: stamp.seq,
                    at,
                    change: applied.change.clone(),
                });
                let mut fx = vec![Effect::Save, Effect::Applied(applied)];
                fx.extend(self.push());
                fx
            }
        }
    }

    fn push(&self) -> Option<Effect> {
        (!self.saved.outbox.is_empty()).then(|| Effect::Push {
            changes: self.saved.outbox.clone(),
        })
    }

    fn pull(&self) -> Option<Effect> {
        Some(Effect::Pull {
            after: self.saved.base_n,
        })
    }

    fn pulled(&mut self, entries: Vec<LogEntry>) -> Vec<Effect> {
        let mut fx = Vec::new();
        let mut changed = false;
        for e in entries {
            // A reply to an older pull: already have it.
            if e.n <= self.saved.base_n {
                continue;
            }
            // A gap: something in between went missing; ask again.
            if e.n != self.saved.base_n + 1 {
                fx.extend(self.pull());
                break;
            }
            if e.rejected.is_none() {
                // The server applied it to the same base with the same core,
                // so it applies here too. If it somehow does not (a device
                // on an older version), keep it as a notice.
                if let Err(r) = apply(
                    &mut self.saved.base,
                    &e.envelope.stamp(),
                    &e.envelope.change,
                ) {
                    let mut e = e.clone();
                    e.rejected = Some(r);
                    fx.push(Effect::Notice(e.clone()));
                    self.saved.notices.push(e);
                }
            } else {
                fx.push(Effect::Notice(e.clone()));
                self.saved.notices.push(e.clone());
            }
            if e.envelope.client == self.saved.client {
                let seq = e.envelope.seq;
                self.saved.outbox.retain(|o| o.seq > seq);
            }
            self.saved.base_n = e.n;
            changed = true;
        }
        if changed {
            self.replay();
            fx.insert(0, Effect::Save);
        }
        fx
    }

    /// Rebuild what to show: the base with the outbox on top. A change that
    /// no longer fits stays in the outbox; the server will refuse it too,
    /// and it comes back as a notice.
    fn replay(&mut self) {
        let mut state = self.saved.base.clone();
        for o in &self.saved.outbox {
            let _ = apply(&mut state, &o.stamp(), &o.change);
        }
        self.state = state;
    }
}
