# Workers prototype (throwaway)

Proves, on the real Workers runtime (`workerd`), the pieces decision 0005
depends on. Findings and the recommendation:
[`docs/research/2026-11-workers-prototype.md`](../../docs/research/2026-11-workers-prototype.md).

Everything that runs on the server is Rust (`src/`). Node is only dev
tooling: `wrangler` runs `workerd` locally, and the checks in `test/` are
clients.

| File | What it is |
|---|---|
| `src/user.rs` | One SQLite-backed Durable Object per user: the sync log of 0003 (push, pull after N, idempotent by `(clientId, clientSeq)`) and hibernating WebSockets that poke the user's devices. |
| `src/mcp.rs` | MCP over Streamable HTTP with `rmcp`'s protocol types and no Tokio. Tools `read_log` and `add_todo`; `add_todo` writes through the same log. |
| `src/passkey.rs` | Passkey registration and sign-in with `webauthn-rs`, state kept in the user's object. |
| `src/bench.rs` | One Argon2id hash at m=19 MiB, t=2. |
| `patches/webauthn-rs-core` | `webauthn-rs-core` 0.6.2, vendored whole (Cargo's `[patch]` needs a full copy) with one change: `SystemTime::now()` panics on wasm32. |

Auth is a stub: sync routes take the user from the path, MCP from
`Authorization: Bearer dev-<user>`. Real sessions and Claude's OAuth flow
are later pieces.

## Run it

```
rustup target add wasm32-unknown-unknown
cargo install worker-build --version 0.9.0 --locked
npm ci
npm run dev          # builds, then serves on http://localhost:8787
npm test             # in another shell; the passkey check needs Playwright
```

CI runs the same in `.github/workflows/prototype-workers.yml`.

## API

Everything in the log is a *change*. On the wire it is a `mutation` when a
device pushes it and an `event` once the log has numbered it.

- `POST /u/<user>/push` `{clientId, mutations: [{clientSeq, name, args}]}` →
  `{results: [{clientSeq, seq, duplicate}], lastSeq}`
- `GET /u/<user>/pull?after=N&limit=M` → `{events, lastSeq}`
- `GET /u/<user>/ws` → WebSocket; receives `{"type":"poke","lastSeq":N}`
  after every push that appended something; `ping` → `pong` without waking
  the object
- `POST /mcp` → JSON-RPC (both protocol eras: `initialize` and
  `server/discover`)
- `POST /u/<user>/passkey/{register,login}/{start,finish}`, test page at
  `/passkey.html`
- `GET /bench/argon2`
