---
model: sonnet
---

Judge every comment the diff adds or changes: does it carry its weight, and
could it say the same in fewer words?

- A comment earns its place when a reader would otherwise get something
  wrong or have to dig for it: why the code has this shape, an invariant
  nothing else shows, a platform quirk it works around (iOS, the service
  worker, pointer events). One that restates the code or its name does not.
- Shorter is better when nothing is lost: cut filler, hedges and
  repetition.
- Clear on first reading to someone new to this code.
- Still true after the change, including existing comments next to code
  the diff changes, and about something that is here.

For each finding, either say why the comment should go, or give the
shorter wording.
