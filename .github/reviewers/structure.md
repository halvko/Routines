---
model: opus
---

Look for structural red flags: code whose shape makes it harder to reason
about, slower, or harder to move to a shared, UI-free core (the plan, in
`docs/decisions/0001` and `0002`, is a Rust core behind one synchronous
`update(event)` that the web app, the server and a native app share).

- Logic in the wrong home: a rule about routines, runs, steps or the backlog
  written in `src/app.js` (the UI) instead of `src/graph.js` or another
  module with no DOM in it. Rules belong where they can be tested and shared
  without a browser.
- Overlapping work along a call path: the same lookup, validation or
  computation done by a caller and its callee, or once per row when once
  per render would do (computing cross-run links inside a loop over runs is
  a hidden quadratic).
- Repeated, similarly shaped code: templates, actions or handlers that
  differ only in a name or a constant, or two ways of doing one thing
  (navigating, showing a dialog, removing a run). Point at the copies.
- Hidden coupling: code that relies on an order, position or shape it does
  not own, such as the order steps were written in, a CSS class used as
  state, or the DOM structure another function produces.
- Checks bolted on afterwards: validating after building, where building it
  differently would rule the mistake out; a special case for one context
  when the rule holds in all of them.
- Guards for the impossible: a fallback for a case nothing can reach. Say
  how the app reaches it; if nothing can, it should not be there.
- A helper with one caller that names nothing the call site did not
  already say.

For each finding, name the places involved and say why it matters here.
These are judgment calls, not rules: report a smell when in doubt, say how
sure you are, and the implementer weighs it.
