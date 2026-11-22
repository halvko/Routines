//! The routines a new user starts with: Morning (with Breakfast inside it)
//! and Travel. `seed.json` is the web app's `seed()` from `src/store.js`,
//! copied as is; `test/seed.test.js` checks the two still agree.

use crate::types::Db;

/// What a new user's data looks like before any change.
pub fn seed() -> Db {
    serde_json::from_str(include_str!("seed.json")).expect("seed.json is valid stored data")
}
