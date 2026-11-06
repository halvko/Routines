# Routines

A phone-first, installable web app for running routines, a backlog and the
day. Plain JavaScript with no build step for now; `docs/design.md` is the
product's source of truth and roadmap.

## After making changes

Always run:
```
npm run check        # unit tests + browser tests
```
When a change adds or changes a screen, add or update its case in
`test/visual/screens.spec.js` and look at `npm run screenshots` output
(light and dark) before pushing.

## Things that are easy to get wrong

- **Saved data is the user's source of truth.** Anything that changes what is
  stored in `localStorage` (`routines.v1`) needs `normalize()` in
  `src/store.js` to keep older data loading correctly.
- **The service worker caches the app.** A new file the app loads goes in
  `SHELL` in `sw.js`, and any change to a shell file bumps the cache version
  there, or phones keep running old code.
- **No `confirm()`/`alert()`**: unreliable in home-screen apps on iOS. Act at
  once and offer Undo (`undoable()` in `src/app.js`).
- Every value put into HTML goes through `esc()`.
- Domain rules belong in `src/graph.js` (no DOM), not in `src/app.js`.

## Workflow

Work on a branch and open a PR; Claude reviewers (`.github/reviewers/`) review
it, and the `review gate` status is green once every reviewer thread is
resolved. The owner merges. Merging to `master` deploys to GitHub Pages.
