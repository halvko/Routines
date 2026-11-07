# Decisions

One file per decision that shapes the code: what was decided, why, and what it
rules out. A decision is changed by a new file that supersedes the old one, not
by editing history away; the old file then says which one replaced it.

Research behind them lives in `docs/research/`.

**How we weigh risk.** This is a personal project, not a business: alpha and
pre-1.0 technology is fine wherever failing means rework (an API changes, a
feature is missing). It is not fine where failing could lose the owner's
data: routines, runs and the backlog are their source of truth, so storage
must have backups and an export path, whatever else we try.

| # | Decision | Status |
|---|---|---|
| [0001](0001-languages.md) | Rust for the core and the server; TypeScript + React in the browser | Accepted |
| [0002](0002-core-interface.md) | The core is one synchronous `update(event)`, fed through one queue | Accepted |
| [0003](0003-sync.md) | Hand-rolled sync: an outbox on the device, one ordered log per user | Accepted |
| [0004](0004-auth.md) | Passkeys first, Argon2id password fallback, opaque session cookies | Accepted |
| [0005](0005-hosting.md) | The server runs on Cloudflare Workers, in Rust, on the Paid plan | Accepted, to be confirmed by a prototype |
