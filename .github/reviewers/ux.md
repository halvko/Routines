---
model: opus
---

Judge how the change works for its user: one person, mostly on a phone,
often with one hand, in the middle of doing something else.

- Every tap target is comfortably thumb-sized (about 44 px), and nothing
  needs precision or a second hand.
- A destructive action happens at once with Undo, not behind a confirm
  popup; `confirm()` and `alert()` are unreliable in home-screen apps on
  iOS. Undo must be able to restore what it says.
- A gesture (swipe, hold) is a shortcut, never the only way: the same action
  is reachable with a visible button or from the hold sheet.
- A refusal or error says why and offers the way forward ("has no steps
  yet" with Edit), never just stops.
- Dead ends: a screen with no way back, a dialog that cannot be cancelled, a
  state the user cannot leave (an empty run, a placement they cannot undo).
- It works offline, and nothing the user does is lost if the phone closes
  the app in the background.
- Navigation: back goes where the user came from; opening a run, routine or
  the editor takes the whole screen.

For each finding, describe the moment the user hits it and what they would
expect instead.
