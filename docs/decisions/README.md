# Decisions

One file per decision that shapes the code: what was decided, why, and what it
rules out. A decision is changed by a new file that supersedes the old one, not
by editing history away; the old file then says which one replaced it.

Research behind them lives in `docs/research/`.

| # | Decision | Status |
|---|---|---|
| [0001](0001-languages.md) | Rust for the core and the server; TypeScript + React only in the browser | Accepted |
| [0002](0002-core-interface.md) | The core is one synchronous `update(event)`, fed through one queue | Accepted |
| [0003](0003-sync.md) | Hand-rolled sync: an outbox on the device, one ordered log per user on the server | Accepted |
| [0004](0004-auth.md) | Passkeys first, Argon2id password fallback, opaque session cookies | Accepted |
| 0005 | Where the server runs | Open: researching Rust hosting |
