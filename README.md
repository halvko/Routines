# Routines

Reusable routines you start on demand and tick off.

- **Steps form a graph**: each step lists what it comes *after*. Anything whose prerequisites are done shows up under "Do now", so independent steps can be done in any order.
- **Sub-routines**: a step can be a whole other routine (Breakfast inside Morning).
- **Options (flags)**: give a step "Only if" `gym` / `!gym` / `abroad`. You pick the options when you start a run; hidden steps are bridged over.

## Developing

```
npm ci
npm start            # http://localhost:8080/Routines/
npm run check        # unit tests (node --test) + browser tests (Playwright)
npm run screenshots  # every screen, light and dark, into ./screenshots
npm run vectors      # rewrite the shared test vectors' expected results
(cd core && cargo test)  # the Rust core against the same vectors
```

The domain rules are being ported to a Rust crate in `core/` (no I/O; later
used by the server and, as WebAssembly, by the app). Until the app uses it,
`src/graph.js` and `core/` implement the same rules and are checked against
the same test vectors in `core/tests/vectors/`.

Data lives in localStorage (Export/Import under Routines). Changes go through
pull requests reviewed by Claude reviewers; see `.github/reviewers/README.md`
and `CLAUDE.md`.

## On your phone

Hosted with GitHub Pages at https://halvko.github.io/Routines/ (Settings → Pages → Deploy from branch `master`, folder `/`).

- **iPhone (Safari):** Share → Add to Home Screen.
- **Android (Chrome):** menu → Install app.

It works offline. Data stays on the device, so use Export/Import to move it.
