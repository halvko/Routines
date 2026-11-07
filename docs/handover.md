# Routines: project handover

Snapshot as of 2026-11-07, for whoever picks the project up next: an organizer
session, a worker session, or a Claude Project. It covers what lives only in
past conversations; everything else is in the repo, and this file points to
it. When it disagrees with the repo, the repo wins.

## 1. What this is

A phone-first, installable web app (PWA) for **routines you start when you need
them**, not on a schedule. Steps form a dependency graph, routines nest, and
**options** (flags such as `gym`, `abroad`) vary the steps for that run. There is
also a plain **backlog** for to-dos with no plan.

- Live: https://halvko.github.io/Routines/ (GitHub Pages, deployed from `master`)
- Repo: https://github.com/halvko/Routines
- Product source of truth and roadmap: `docs/design.md`
- Technical decisions: `docs/decisions/` (rationale in `docs/research/`)
- How to work in the code: `CLAUDE.md`

## 2. The owner and how they like to work

- **Single user, the owner.** They use it on an iPhone, installed to the home
  screen. Editing from a computer comes later; the phone stays the main entry
  point.
- **Motivation:** frustrated by to-do apps that assume a fixed cadence or the
  same day every day. The app must be flexible: templates and options, not
  schedules; nothing guilt-inducing.
- **Languages:** most comfortable in **Rust**, TypeScript second, and
  **does not want JavaScript on the server**.
- **Risk:** alpha and pre-1.0 technology is welcome where failure means rework.
  It is not acceptable where it could lose their data.
- **Process:** wants agent-written code reviewed by agent reviewers (as in their
  `halvko/must` project), PRs merged by them, and **research delegated to a
  researcher agent** rather than done inline.
- **Style:** thinks out loud and refines ideas over several messages; good
  ideas often come from that. Push back with reasons. Messages are often
  dictated, so a strange word may be a transcription slip (once "missions"
  was meant as "animations", though missions then turned out to be a good idea
  too).
- They like concrete proposals with a recommendation, then a short list of
  decisions for them.

## 3. Current state

**The app** is plain JavaScript with no build step (`index.html`, `src/`,
`style.css`, `sw.js`). It works offline, data lives in `localStorage`, and
Export/Import is under Routines. Built so far:

- Routines with steps, "after" dependencies, sub-routines and "only if"
  options. Hidden steps are bridged, so their prerequisites pass through.
- **Today:** several runs at once, in one mixed "Do now" list (with a tag
  naming each step's routine) or grouped by routine. Runs finish by
  themselves.
- **Placing:** a run or pulled to-do waits *after*, or holds back *before*, a
  step of another run. Loops across runs are refused.
- **Backlog:** capture via the always-visible `+`, a star for priority, tick
  in place, or pull into Today.
- **Gestures:** swipe left to drop or delete (always with Undo); hold, or
  right-click on a computer, for a sheet of every action on that row.
- **Focus mode:** a run, routine or the editor takes the whole screen, and
  back returns to where you came from. Reopening the app within an hour
  resumes the screen that was open.
- A routine page that shows its steps, with Edit inside it.

**Engineering:**

- `npm run check` runs 15 unit tests and 45 Playwright browser tests at phone
  size. CI runs them on every PR and on pushes to `master`.
- **Eight Claude reviewers** run on every PR (`.github/reviewers/`):
  correctness, product, ux, visual (with screenshots of head and base in
  light and dark), structure, tests, naming-and-wording and comments. They
  use `must`'s workflow and `review gate` status. Briefs are plain files;
  edit one to teach that reviewer.
- PR #1 (CI and reviewers): merged.
- **PR #2 (decisions and research): open.** It is the first PR the reviewers
  run on, so it also tests the setup. Its threads need answering.
- Unknown: whether the `CLAUDE_CODE_OAUTH_TOKEN` secret and the `review` label
  exist yet (`claude setup-token`, then repo Settings → Secrets → Actions),
  and whether `master` is protected (requiring CI and the review gate).

## 4. Decided stack (summary of `docs/decisions/`)

| # | Decision |
|---|---|
| 0001 | Rust core crate with every domain rule and no I/O; Rust server linking it natively; React + TypeScript web app calling it as WASM. Crux only once a native app is concrete. |
| 0002 | The core is one synchronous `update(event) -> (state, effects)` that never waits. Each shell feeds it through one serial queue and owns connections. The core ignores stale replies (request ids or versions). |
| 0003 | Hand-rolled sync: an outbox on the device, and one ordered log per user on the server, which applies the same core. Per-field last-write-wins for templates and backlog; an append-only event log for runs. |
| 0004 | Passkeys first. Password fallback per NIST SP 800-63B-4 (at least 15 characters, no composition rules), HIBP check, Argon2id (19 MiB, t=2). Opaque `__Host-` session cookies, no JWTs. Hand-rolled. |
| 0005 | Cloudflare Workers in Rust (`workers-rs`), Paid plan (~$5/month), one SQLite Durable Object per user, one origin for app, API, sign-in and MCP. **Pending a prototype.** Fly.io is the runner-up. |

Consequences worth knowing:

- The app must move from GitHub Pages to the Worker's origin when the server
  arrives, because Safari blocks cross-site cookies. `localStorage` does not
  follow, so the owner's data moves by Export → Import, or by a one-time
  upload built for it.
- No Rust crate implements the OAuth server Claude's connector needs, so it is
  hand-rolled (about 8 endpoints). The exact subset is in
  `docs/research/2026-11-rust-server-hosting.md`.

## 5. Proposed way of running the project (not yet agreed)

- **GitHub issues hold the state**: one issue per work item, with what to do,
  the decisions that apply, a definition of done, and "blocked by #n". This
  way no session's memory is load-bearing.
- **An organizer session** writes issues, starts worker sessions when their
  dependencies are done, checks progress on a schedule (about twice a day, and
  when asked), keeps issues current, and tells the owner what needs them.
  Workers that finish cleanly do not notify the organizer, which is why the
  check-ins exist.
- **One worker session per issue**, on its own branch. It opens a PR and owns
  it through the reviewer rounds, then the owner merges.
- **Start with two workers, not everything**: each session and each review
  round costs tokens.

Proposed issues:

```
Prototype: Rust on Workers ─┬─► Sync
  (confirms 0005)           ├─► Auth (passkeys, password)
                            └─► MCP server + OAuth for Claude
Rust core: port today's rules,
  tests as JSON vectors ──────► React + TS web app on the WASM core
                                (same behaviour as today)
```

The prototype and the core touch nothing in common and can run in parallel
first. The prototype must show, on the real runtime: a per-user Durable
Object with SQLite, `webauthn-rs` passkeys and one Argon2id hash within the CPU
limit, a WebSocket push, one MCP tool behind hand-rolled OAuth that Claude can
call, and Worker Previews for a Rust project. If it fights back for more than
about a weekend, switch to Fly.io (see 0005).

## 6. Open questions for the owner

1. Freeze the current JavaScript app to bug fixes only during the rewrite?
   (Recommended: yes, so features are not built twice.)
2. Agree to the way of working in section 5, create the issues, and start the
   first two workers?
3. Should a session watch PR #2 and answer its reviewer threads?

## 7. Loose ends and known issues

- **"Skip" and "Drop" do the same thing** (mark a step skipped). Pick one word.
  The naming reviewer's brief flags it.
- **Abandon on an empty run failed on the owner's phone** but could not be
  reproduced in Chromium. The fix removed `confirm()` (unreliable in iOS
  home-screen apps) and stopped empty runs from being created. That
  `confirm()` was the cause is a hypothesis; ask the owner whether it is fixed.
- The **visual reviewer's screenshot step** has not run in CI yet. PR #2 is
  its first chance, though a docs-only PR barely exercises it.
- Seed routines (Morning, Breakfast, Travel) are examples. The owner's real
  data is on their phone, so never "fix" their routines in code.
- The owner noted Travel shows why **check-in online should come before
  packing**. In their data that is an edit they can make; as a product idea it
  feeds multi-day routines (below).

## 8. Ideas in the backlog of the product (see `docs/design.md`)

- **Day as the base routine** (stages: Wake up → Leave home → Commute home →
  Back home → Bedtime). Everything attaches to stages; the weekend flag
  removes the commute stages. "Routines all the way down."
- **Leftovers:** a new day starts with one tap and is never blocked. Unfinished
  carry-over items land in the backlog, marked "from yesterday". Each routine
  has a carry-over or lapse setting, defaulting to carry over when created.
- **Multi-day routines:** an anchor date ("departure: Friday") and per-step
  timing ("2 days before").
- **Gamification without guilt:** satisfying tick animations and celebrations
  first. Then gentle counts, earning self-chosen vices, optional missions, and
  categories with badges (a green badge for mental health; "one
  mental-health thing a week").
- **Later:** editing from a computer, an MCP server so Claude can manage the
  owner's tasks, AI-drafted routines ("I need to do X" → suggested steps), and
  Web Push reminders (iOS 16.4+, home-screen apps only).

## 9. Where things are

| Path | What |
|---|---|
| `src/graph.js` | Domain rules (pure): expand, bridging, ready/waiting, placing, cycles |
| `src/app.js` | UI, actions, navigation, dialogs, undo |
| `src/gestures.js` | Swipe and hold |
| `src/store.js` | Seed data, `normalize()` for older saved data |
| `sw.js` | Service worker (network-first; bump `CACHE` on shell changes) |
| `test/*.test.js` | Unit tests for `graph.js` |
| `test/e2e/` | Browser tests; `helpers.js` has `step()`, `swipe()`, `hold()` |
| `test/visual/` | Screens captured for the visual reviewer |
| `.github/reviewers/` | Reviewer briefs; `README.md` explains the rounds and the gate |
| `docs/` | `design.md`, `decisions/`, `research/`, this file |
