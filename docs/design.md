# Design notes

Living document of where Routines is heading. Decisions marked ✅ are built.

## Core ideas

- **Routines are templates, not schedules.** You start a *run* when you need it; nothing assumes a cadence.
- **Steps form a graph.** A step waits on the steps it comes *after*; anything whose prerequisites are done is ready, so independent steps happen in any order. ✅
- **Routines nest.** A step can be a whole other routine. ✅
- **Flags are the templating.** Steps (and dependencies) can be "only if `gym`" / `!gym`; flags are chosen when starting a run. Hidden steps are bridged so dependencies still flow. ✅
- **Runs are snapshots.** Editing a routine never rewrites a run already started. ✅
- **Routines all the way down.** The day itself is a routine; everything else attaches to it.

## Build order

1. **Today with several runs.** ✅ One mixed "Do now" list across all running routines, each step tagged with its routine. Toggle to group by routine instead — keep whichever wins in practice.
2. **Attaching runs.** Any run can be attached *before* or *after* any step of another run. Its entry steps then wait on that step (or that step waits on its exit steps). E.g. "After gym" after *Commute home*; "Pack gym bag" before *Leave home*.
3. **Day as the base routine.** A Day template whose steps are the stages: Wake up → Leave home → Commute home → Back home → Bedtime. Each day spawns a run of it. Flags like `weekend` remove stages (e.g. `!weekend` on the commute ones) and attached things bridge to neighbouring stages. One-off single steps can be added into a stage ("Call the dentist" after *Back home*). Planning = attaching to a future day ahead of time.
4. **Leftovers inbox.** Starting a new day is one tap (or automatic) and never blocked by yesterday. Unfinished carry-over items go into an inbox, shown as a small badge (`4 left from yesterday`). It accumulates across days and nothing disappears without a decision. Actions per item: **Today** (pull into a stage), **Later** (into a future day), **Done**, **Drop**. ("Handled elsewhere", e.g. put in the calendar, is just Done.)
5. **Carry-over setting per routine.** "If left unfinished: carry over / let lapse."
   - Default for *newly created* routines: **carry over** (wrongly kept = irritating; wrongly dropped = forgotten agreement).
   - Snapshotted at instantiation — changing it affects the next run, not past ones.
   - One-off steps added to a day carry over.
   - Sub-routine steps follow the routine they came from (Morning lapses, a "Pay bills" inside it carries over). Revisit if wrong in practice.
   - Morning/Breakfast-type routines: lapse. Per-step override only if missed in practice.
6. **Dropping is first-class.** Drop a single step or a whole run any time, ideally a swipe. Not a failure state.

## Later

- Rewrite in TypeScript once the shape has settled. Plain JS (no build step) is deliberate for now, to keep experimenting cheap.
- Backend + login so it can be edited from a computer (phone stays the main entry point).
- AI drafting: "I need to do X" → suggested routine you then edit. Needs the backend to keep the API key off the phone.
- Companion runs: a flag spawns linked routines already attached (starting Morning with `gym` also spawns Pack gym bag + After gym).
- Day templating by weekday (weekend flag set automatically, etc.).

## Open questions

- Does a new day spawn automatically on first open, or via a "Start day" tap?
- Today: mixed vs grouped — decide after real use.
