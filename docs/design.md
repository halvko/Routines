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
2. **Attaching runs.** ✅ Any run can be attached *before* or *after* any step of another run. Its entry steps then wait on that step (or that step waits on its exit steps). E.g. "After gym" after *Commute home*; "Pack gym bag" before *Leave home*.
3. **Day as the base routine.** A Day template whose steps are the stages: Wake up → Leave home → Commute home → Back home → Bedtime. Each day spawns a run of it. Flags like `weekend` remove stages (e.g. `!weekend` on the commute ones) and attached things bridge to neighbouring stages. One-off single steps can be added into a stage ("Call the dentist" after *Back home*). Planning = attaching to a future day ahead of time.
4. **Leftovers inbox.** Starting a new day is one tap (or automatic) and never blocked by yesterday. Unfinished carry-over items go into an inbox, shown as a small badge (`4 left from yesterday`). It accumulates across days and nothing disappears without a decision. Actions per item: **Today** (pull into a stage), **Later** (into a future day), **Done**, **Drop**. ("Handled elsewhere", e.g. put in the calendar, is just Done.)
5. **Carry-over setting per routine.** "If left unfinished: carry over / let lapse."
   - Default for *newly created* routines: **carry over** (wrongly kept = irritating; wrongly dropped = forgotten agreement).
   - Snapshotted at instantiation — changing it affects the next run, not past ones.
   - One-off steps added to a day carry over.
   - Sub-routine steps follow the routine they came from (Morning lapses, a "Pay bills" inside it carries over). Revisit if wrong in practice.
   - Morning/Breakfast-type routines: lapse. Per-step override only if missed in practice.
6. **Backlog (plain to-do).** ✅ (leftovers part pending slice 4) Somewhere to dump things with no plan: single items, no date, no stage. Instant capture via a `+` reachable from anywhere. Optional priority, set later (start with a star; maybe now-ish / normal / someday). Pull an item into Today, a stage, or a future day — same action as for leftovers. An item can grow into a routine (natural place for AI: "plan Anna's birthday" → suggested steps).
   - **Leftovers live here too:** unfinished carry-over items land in the backlog marked "from yesterday", badged until seen. One source of truth for "didn't get to it" and "will get to it". Dumped items never nag; new leftovers badge until reviewed.
7. **Dropping is first-class.** ✅ Swipe left to delete/drop (with Undo); hold (or right-click) for every action on a row. Drop a single step or a whole run any time, ideally a swipe. Not a failure state.

## Multi-day routines

Some routines span days. Travel: check in online (usually before packing starts) days ahead, pack the evening before, lock up on the day.

- A run can have an **anchor date** chosen when starting it ("departure: Friday").
- Steps get an optional **timing relative to the anchor**: "2 days before", "the day before", "on the day". Default: no timing = available as soon as its prerequisites are done.
- Until its day, a step shows as *Upcoming · Thu* rather than in Do now; on its day it appears in Today like anything else.
- Timing and "after" combine: Pack waits on Check in **and** for the day before. Ordering stays the graph; dates only gate.
- Once Day runs exist (slice 3), a timed step lands in that day's run (e.g. after *Back home* the evening before) — routines all the way down.
- The run itself already persists across days; what's new is gating steps by date.

## Later

- Rewrite on the decided stack (`docs/decisions/`): a Rust core shared by a Rust server and a React + TypeScript web app. Plain JS (no build step) stays until then, to keep experimenting cheap.
- Backend + login so it can be edited from a computer (phone stays the main entry point). Sync, auth and hosting are decided (`docs/decisions/0003`–`0005`); a prototype on Cloudflare Workers confirms the hosting first.
- **MCP server** on that backend, so Claude can be connected as an integration and manipulate tasks directly: read Today, add to the backlog, tick/drop steps, start and place routines, draft/edit routines. Needs the backend (data must live somewhere Claude can reach) and auth. Same tool surface the AI drafting feature would use.
- AI drafting: "I need to do X" → suggested routine you then edit. Needs the backend to keep the API key off the phone.
- Companion runs: a flag spawns linked routines already attached (starting Morning with `gym` also spawns Pack gym bag + After gym).
- Day templating by weekday (weekend flag set automatically, etc.).

## Gamification (later, start without)

Goal: a little happiness from checking things off, never guilt. Fits the "no fixed cadence" philosophy, so no streaks that punish a missed day.

- Animations are the core: make ticking itself satisfying (check animation, haptic tap) and celebrate a finished routine or day with something a bit bigger.
- Gentle counts: steps done today, a week view. Rewards what happened; never shows what was missed.
- Missions (maybe — an accidental idea worth keeping): occasional optional challenges ("finish Morning before 8 three times this week"). Ignoring one costs nothing.
  - Missions can target a *category* rather than specific steps: "one mental-health thing a week" is met by any step carrying that badge, wherever it lives.
- Categories: tag steps/routines (mental health, movement, social, admin…), each with a colour and small badge shown wherever the step appears (e.g. green for mental health). Useful on its own: Today shows at a glance if it's all admin and no green.
- Earning vices: completed steps earn points that can be spent on self-chosen treats ("an episode", "dessert"). Allowing yourself things, explicitly.

## Open questions

- Does a new day spawn automatically on first open, or via a "Start day" tap?
- Today: mixed vs grouped — decide after real use.
- A run can currently be placed relative to one step only. Enough?
