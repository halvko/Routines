# Workers prototype (throwaway)

**This is not the real server.** It is a throwaway prototype for issue #3,
checking whether decision 0005 (Cloudflare Workers in Rust, one SQLite
Durable Object per user) works on the real runtime. Delete it once the real
server exists. Nothing here is hardened: one shared test page, user names
in tokens, no rate limits, no CSRF token on the consent form.

## Results

Everything below ran on **workerd locally** (`wrangler dev`, Wrangler 4.157,
`worker` 0.8.7, Rust 1.97), driven by `test/prototype.spec.js`. The deployed
checks still need a Cloudflare token (see "Still to do").

| # | What | Result |
|---|---|---|
| 1 | Rust Worker → per-user Durable Object → SQLite | **Worked.** `get_by_name(user)` routes; tables are created in the constructor with `sql().exec`. |
| 2 | Passkeys with `webauthn-rs` 0.6.2 | **Worked with a workaround.** Finishing registration panicked: `webauthn-rs-core` calls `std::time::SystemTime::now()`, which wasm32-unknown-unknown does not have. A small patch (`patches/`: four lines of code and a `js-sys` dependency) reads the clock from JavaScript instead. Sign-in needed nothing. |
| 3 | One Argon2id hash (m=19456 KiB, t=2, p=1) | **Worked.** About 32 ms per request locally (median of 10, including HTTP). CPU time on Cloudflare: not measured yet. |
| 4 | WebSocket push with the hibernation API | **Worked**, including after hibernation: with the socket idle for 20 s the object was evicted, and a fresh instance pushed through the old socket. |
| 5 | One MCP tool behind hand-rolled OAuth | **Worked against a scripted client playing Claude's part.** **Not yet tried with Claude itself**, which needs a public URL. |
| 6 | Worker Previews for a Rust project | **Not tried.** Needs a Cloudflare account. |

### Details

**1. Durable Object and SQLite.** `UserObject` (`src/user.rs`) keeps
routines, passkeys, sessions and OAuth state in its own SQLite. `SqlCursor::to_array`
deserializes rows straight into serde structs. The Worker routes by user
name, and tokens are `<user>.<secret>` so routing needs no lookup (the real
server should put a user id there, not the name).

**2. Passkeys.**
- The browser uses `PublicKeyCredential.parseCreationOptionsFromJSON` and
  `credential.toJSON()`. `webauthn-rs` accepts that JSON as it is.
- Ceremony state lives in SQLite between start and finish. That needs the
  `danger-allow-state-serialisation` feature, which is the normal choice for
  a server that does not keep memory between requests.
- The clock panic is the only wasm32 problem found. It is in
  `webauthn-rs-core/src/core.rs` (registration only; attestation certificate
  checks use the time). `patch-webauthn.sh` downloads the published crate,
  checks its SHA-256, and applies the patch; Cargo's `[patch.crates-io]`
  points at it. For the real server, propose the fix upstream (for example
  with `web-time`) or keep the patch.

**3. Argon2id.** `GET /hash` hashes once in the Worker. Locally (2.3 GHz
Xeon) round trips were 30–36 ms, median 32 ms, in line with the research's
37 ms in Node. Within Workers Paid's 30 s CPU limit by far; above the Free
plan's 10 ms.

**4. Push.** `/api/ws` upgrades through the Worker to the user's object,
which accepts the socket with `state.accept_web_socket` (hibernatable) and
sends to `state.get_websockets()` on each change. The test opens two pages
(two devices): the other one saw a change 46–81 ms after the click, locally.
Test 4b (`HIBERNATION_WAIT=20`) waits until the object is evicted and checks
that the push comes from a new instance (its `born` time changed).

**5. OAuth and MCP.** About 240 lines in `src/oauth.rs` plus the code and
token handling in `src/user.rs`, and 110 lines of MCP in `src/mcp.rs`.
- OAuth, all checked by the test: protected resource metadata (RFC 9728) at
  both well-known paths; authorization server metadata (RFC 8414); dynamic
  client registration with a redirect allowlist (Claude's callbacks and
  loopback); `/authorize` with S256 PKCE, `resource` checking, sign-in by
  passkey and a consent page; `/token` with single-use codes and rotating
  refresh tokens (reusing a spent one revokes the whole family); and `401`
  with `WWW-Authenticate: Bearer resource_metadata=…` on the MCP endpoint.
- Client ID Metadata Documents (CIMD) are implemented but untested: they
  need a client document at an HTTPS URL. `global_fetch_strictly_public`
  keeps that fetch off private addresses.
- MCP uses `rmcp` 3.9's protocol types (`InitializeResult`,
  `DiscoverResult`, `ListToolsResult`, `CallToolResult`, `Tool`) with a
  hand-written JSON-RPC dispatcher, since `rmcp`'s HTTP transport needs
  Tokio. It answers both protocol eras: `initialize` (2025-06-18 and
  2025-11-25) and 2026-07-28's `server/discover` with the version in
  `_meta`. Responses are plain JSON; there is no SSE stream.
- `list_routines` reads the user's SQLite.

### Measurements

| What | Value |
|---|---|
| WASM size | 1,500,970 bytes (1.43 MiB), 466,628 bytes gzipped, plus 32 KB of JS glue. Workers Paid allows 10 MB compressed. Built with `opt-level = "z"`, LTO and `wasm-opt`. |
| Argon2id, round trip | 32 ms median, locally. |
| Argon2id, CPU time on Cloudflare | Not measured yet. |
| New Durable Object, first request | 12 ms locally (not meaningful for production). |
| Cold wake of an evicted Durable Object | Not measured yet; needs a deploy (`measure.mjs 120`). |
| Push after hibernation, locally | 29 ms from the HTTP request to the message arriving. |

## Recommendation

**Confirm 0005, once the deployed checks pass.** Every item that could be
tried locally works in Rust. The only problem was one clock call, fixed by a
small patch, well inside the time box issue #3 set (about a weekend). Nothing so far
points at Fly.io.

What could still change that: Claude failing to connect for a reason that
is specific to Workers (unlikely, since the protocol code is ordinary Rust),
Previews not supporting a custom build command, or a cold wake much slower
than the ~1.5 s third-party figure in the research.

## Still to do (needs a Cloudflare token)

1. `npm run deploy`, then run the tests against it:
   `PROTO_URL=https://… HIBERNATION_WAIT=120 npx playwright test`.
2. `PROTO_URL=https://… node measure.mjs 120` for cold-wake latency.
3. CPU time per hash from Workers Observability, or the GraphQL Analytics
   API (`workersInvocationsAdaptive`, `quantiles.cpuTimeP50`).
4. Add `https://…/mcp` as a custom connector in Claude (Settings →
   Connectors), sign in with a passkey, and ask Claude to list routines.
5. `npx wrangler preview --name test`, and check that a preview's Durable
   Objects are separate (a user registered on the preview is unknown on the
   main deployment).

## Running it

```sh
rustup target add wasm32-unknown-unknown
cargo install worker-build --version 0.8.7
npm ci
npx playwright test     # starts wrangler dev and runs items 1-5
```

`wrangler dev` runs `patch-webauthn.sh` and `worker-build` itself
(`[build]` in `wrangler.toml`). To build with Cargo directly, run
`./patch-webauthn.sh` first.
