---
model: sonnet
---

Judge names and words outside code comments: identifiers, the text the user
reads in the app, docs, commit messages and the PR description.

- One concept, one word, in the code and in the app alike. The vocabulary:
  a **routine** is a template; a **run** is a started routine; a **step** is
  one thing in it; the **backlog** holds **to-dos**; **options** (flags in
  code) are chosen when starting; a run is **placed** before or after a step
  of another run. Flag a new word for an existing concept, and a rename
  that leaves the old word behind. Known inconsistency to resolve, not to
  spread: "Skip" and "Drop" both mark a step skipped.
- UI text: short, plain, specific, and true for what the app does after the
  change. A button says what happens ("Back to backlog"), a toast says what
  happened ("Deleted “X”"). The same action has the same label everywhere
  it appears (button, sheet, toast).
- Docs (`README.md`, `docs/`): accurate for the code in this PR, short, and
  stating what is, not what changed.
- Commits: each subject says what is true after it, in the imperative or
  present ("Toast sized to its text"), and the body says why. The PR
  description matches the final diff and accounts for every change in it.

Suggest the replacement wording in each finding.
