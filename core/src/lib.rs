//! The rules of Routines, with no I/O: routines and their steps, flags and
//! bridging of hidden steps, sub-routines, runs and placing runs relative to
//! each other.
//!
//! A port of the web app's `src/graph.js`, checked against the same test
//! vectors (`tests/vectors/`), whose expected results come from the
//! JavaScript.

mod cond;
mod editor;
mod expand;
mod runs;
mod types;

pub use cond::{Term, collect_flags, cond_holds, parse_cond};
pub use editor::{includes, would_cycle};
pub use expand::{ExpandError, expand};
pub use runs::{
    Ctx, Links, Runs, Waiting, cross_links, has_cycle, is_finished, ready, roots, sinks, waiting,
};
pub use types::*;
