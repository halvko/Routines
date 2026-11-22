//! The rules of Routines, with no I/O: routines and their steps, flags and
//! bridging of hidden steps, sub-routines, runs and placing runs relative to
//! each other; and every change to the data, how devices sync those changes,
//! and the MCP tools Claude uses to make them.
//!
//! - The rules (`cond`, `expand`, `runs`, `editor`) are a port of the web
//!   app's `src/graph.js`, checked against the same test vectors
//!   (`tests/vectors/`), whose expected results come from the JavaScript.
//! - [`Change`] and [`apply`]: every way the data can change, as plain data,
//!   applied the same way on a device and on the server.
//! - [`Client`] and [`Log`]: the device and server sides of sync (decision
//!   0003). [`Client::update`] is the core's one entry point on a device
//!   (decision 0002): an [`Event`] in, [`Effect`]s out.
//! - [`today`], [`open_backlog`]: what the screens and MCP read.
//! - [`tools`]: the MCP tools, generated from [`Change`].

mod change;
mod cond;
mod editor;
mod expand;
mod runs;
mod seed;
mod sync;
mod tools;
mod types;
mod view;

pub use change::{
    Applied, Change, Kind, Placement, Rejected, Stamp, TodayMode, apply, live_routines, live_runs,
};
pub use cond::{Term, collect_flags, cond_holds, parse_cond};
pub use editor::{includes, would_cycle};
pub use expand::{ExpandError, expand};
pub use runs::{
    Ctx, Links, Runs, Waiting, cross_links, has_cycle, is_finished, ready, roots, sinks, waiting,
};
pub use seed::seed;
pub use sync::{Client, Effect, Envelope, Event, Log, LogEntry, Saved};
pub use tools::{Annotations, READ_TOOLS, Tool, change_from_tool, tools};
pub use types::*;
pub use view::{RunningRun, Today, TodayStep, open_backlog, today};
