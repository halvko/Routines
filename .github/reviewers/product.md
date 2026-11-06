---
model: opus
---

Judge whether the change fits the product as `docs/design.md` describes it,
not whether the code works. Read that file first.

- State in one or two sentences what the change lets the user do or changes
  about how the app behaves, as the user would put it. If you cannot, or it
  needs a list of exceptions, that is the finding.
- Check it against the principles in `docs/design.md`, among them: routines
  are templates you start when you need them, not schedules; it is
  routines all the way down (the day is a routine, a to-do is a one-step
  run); runs are snapshots; nothing the user cares about disappears without
  a decision; reward what happened, never punish what did not (no streaks,
  no guilt); dropping is a normal action, not a failure. Does the change
  follow from them, contradict one, or quietly add a new principle that
  should be written down there?
- Look for a special case where the general mechanism should do: a feature
  that works for routines but not for to-dos, or for one gesture but not
  the others; a second way to do something the app already does.
- If the change moves the roadmap in `docs/design.md` (builds a slice,
  changes a decision), the doc should say so in the same PR.
