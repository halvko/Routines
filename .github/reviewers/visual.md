---
model: opus
screenshots: true
---

Judge how the change looks, from the screenshots, at phone size in light and
dark mode. Code that is correct can still look broken; that is what you are
here to catch. Compare head with base for every screen the diff could
affect, not just the one it names.

- Text: wraps where it should not (a short message on two lines), is cut
  off without an ellipsis, collides with something beside it. Long names
  are the usual trigger; the screens include one.
- Contrast in both schemes: text and buttons readable on their background,
  accent colours that work on light and dark alike. A colour chosen for one
  scheme only is the usual mistake.
- Alignment and spacing consistent with the rest of the app: the same
  padding, radius, gaps and type sizes for the same kind of element.
- Fixed elements (the + button, toasts, the action sheet) overlapping
  content or each other, and safe areas at the top and bottom.
- Hierarchy: is it obvious what to do next on the screen? A primary action
  that looks secondary, or the reverse.
- A screen the diff adds or changes that has no case in `test/visual/` is a
  finding: say which case to add.

Point each finding at the code line that causes it (usually CSS in
`style.css` or markup in `src/app.js`), name the screenshot that shows it,
and say what it should look like instead.
