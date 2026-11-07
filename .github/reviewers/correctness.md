---
model: opus
---

Judge whether the change is correct. This app is its owner's source of truth
for what they have to do: losing, corrupting or silently dropping their data
is the worst bug it can have.

- Saved data (`localStorage` key `routines.v1`, loaded through `normalize()`
  in `src/store.js`): does data saved by the previous version still load and
  mean the same thing? A new field needs a default for old data; a renamed or
  removed one needs a migration in `normalize()`. Data that fails to load
  must never be overwritten by a fresh seed.
- Undo restores a snapshot of everything: is anything changed outside the
  snapshot (a toast, navigation, a second write) that Undo then misses or
  overwrites?
- References that can dangle: a run's `attach` pointing at a deleted run or
  step, a backlog item's `runId`, a sub-routine step whose routine is gone,
  a dependency on a removed step. Follow each delete the diff adds or touches
  and say what still points at the deleted thing.
- Runs are snapshots of a routine at start: editing the routine must not
  change a run already started.
- The graph rules (`src/graph.js`, and their Rust port in `core/src/`):
  hidden steps bridge their prerequisites,
  sub-routines splice in between their neighbours, placements across runs
  never form a loop, a run finishes exactly when every step is done or
  skipped. Test the change against nested sub-routines, a routine whose
  steps are all hidden, the same routine included twice, and a run placed
  relative to a step that is already done.
- The Rust core (`core/`) must behave exactly like `src/graph.js` until the
  JavaScript is retired: same results, same order of steps and blockers,
  same error messages. Look for differences the vectors would miss: a map
  iterated in a different order (the core keeps stored order with
  `IndexMap`; a `HashMap` there changes results), string handling (JavaScript
  sorts and splits by UTF-16 and its own whitespace), a missing or empty
  field treated differently from `undefined` or `''`.
- The core's types (`core/src/types.rs`) mirror the stored JSON: every
  field the app writes, optional where older data may lack it, and unknown
  fields kept in `extra` so saving through the core drops nothing. A field
  made required, renamed or retyped can make existing data fail to load.
- Every value interpolated into HTML goes through `esc()`.
- The service worker (`sw.js`): a new file the app loads must be in `SHELL`,
  and a change to any shell file needs the cache version bumped, or phones
  keep running the old code offline.

Give each finding the steps or the saved data that show the failure.
