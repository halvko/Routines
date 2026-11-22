# Workers prototype: findings and the hosting recommendation

Date: 2026-11-22. Code: `prototype/workers/`. Confirms (or not) decision
0005, with the owner's standing requirement that **multi-device sync and MCP
are first class**.

## Recommendation

**Keep Cloudflare Workers in Rust (0005).** Everything sync and MCP need ran
on the real Workers runtime (`workerd`, locally) on the first day, in about
600 lines of Rust, with two small wasm32 snags, both fixed. Nothing fought
back for anywhere near the "weekend" budget 0005 set.

**Fallback: AWS Lambda + DynamoDB + API Gateway WebSockets**, as the owner
prefers, instead of Fly.io. It costs more glue code than Fly (per-user
ordering and fan-out have to be built), but its storage is managed with
point-in-time recovery, and nothing is a single box we look after. See the
comparison below.

Still unproven, and needing a real deploy: cold-wake latency of a Durable
Object, Worker startup time with a 1.8 MB wasm, Worker Previews for a Rust
project, and Claude connecting through OAuth (OAuth isn't built yet).

## What was proven, and how

All on `wrangler dev` 4.166 (which runs `workerd`, the Workers runtime),
`worker` 0.9.0, Rust 1.97. `npm test` in `prototype/workers` runs both
scripts; CI runs them on every PR that touches the prototype.

| 0005 prototype item | Result | Check |
|---|---|---|
| 1. Per-user Durable Object storing data in SQLite | **Works.** The sync log of 0003: push numbers each change, a retry with the same `(clientId, clientSeq)` gets its first number back, pull returns everything after N, each user has their own object. Data survived a runtime restart. | `test/e2e.mjs`, 4 checks |
| 2. Passkeys with `webauthn-rs`, one Argon2id hash | **Works** after a one-line patch (below). Registration and sign-in in Chromium with a virtual authenticator; another user without passkeys is refused. Argon2id at m=19 MiB, t=2: **27–40 ms** in-request, about 30 ms added to the round trip. | `test/passkey.mjs`, e2e "Argon2id" |
| 3. WebSocket push from the object to a browser | **Works.** A push from "phone" reaches "laptop" as `{"type":"poke","lastSeq":N}` about 9 ms after the push starts. Sockets use the hibernation API, and `ping` is answered by the runtime without waking the object. | e2e, 2 checks |
| 4. One MCP tool Claude can call | **Works without OAuth.** `rmcp` 3.11's protocol types, no transport, no Tokio. Both protocol eras: `initialize` (2025-11-25) and `server/discover` (2026-07-28). `add_todo` writes through the same log, so Claude's change pokes the user's devices like a phone's would; `read_log` sees every device's changes. OAuth is still to build. | e2e, 6 checks |
| 5. Worker Previews for Rust | **Not tried**: needs a Cloudflare account. | — |

Why this makes sync and MCP first class: the user's Durable Object is the
**one door** for every write. A device push and an MCP tool call go through
the same function, get numbers from the same counter, and trigger the same
poke. There is no second write path for MCP to drift from, and the core
(0002) plugs in at one marked spot (`User::push`).

## Snags found (all small)

1. **`webauthn-rs-core` 0.6.2 calls `SystemTime::now()`**, which panics on
   wasm32. It compiled fine (the earlier research could only show that);
   it failed at runtime on the first registration. Fixed with a one-line
   `[patch]` taking the time from `js_sys::Date::now()`
   (`prototype/workers/patches/`). Worth upstreaming; a clock parameter
   would do.
2. **A panic inside a Durable Object aborts the instance, and the request
   hangs** until the client gives up (seen with the bug above). The server
   must not panic: no `unwrap`/`expect` on request paths, a clippy lint for
   it, and every handler returning `Result`. Building with workers-rs's
   panic-unwind option is worth trying in the real server.
3. **Randomness on wasm32** needs features switched on: `getrandom`'s
   `wasm_js` and `uuid`'s `js` (for `rmcp`). One line each.
4. **No `transactionSync` wrapper** in workers-rs. Not needed: SQL calls
   with no `await` between them are committed together by the Durable
   Object, and nothing is confirmed to the client before it is durable.
5. **Timers don't advance inside a request** on deployed Workers (a
   Spectre mitigation), so in-request timings only work locally.

Size and build: the release wasm is **1.8 MB (578 KB gzipped)** with
webauthn, argon2 and rmcp, well under the 10 MB Paid limit (3 MB on Free).
A clean release build takes about 50 s.

## Workers vs Lambda vs a native micro-VM (Fly.io)

The owner asked whether a native Rust micro-VM gives better performance per
dollar, and prefers Lambda over Fly.io as the fallback.

**Performance.** Native Rust is faster per CPU-millisecond than wasm
(Argon2id: about 20 ms native vs 27–40 ms here), but a sync request uses
well under a millisecond of CPU. What users feel is the network and the
storage hop. Workers and Fly keep SQLite **in the same process** as the
code (microseconds); Lambda reaches DynamoDB over the network for each
call (single-digit milliseconds each, two or three per push).

**Assumptions for cost.** Per active user per month: about 3,000 sync
requests (100 a day), one device holding a WebSocket 8 hours a day (14,400
connection-minutes), and 3,000 pokes. US list prices as of November 2026;
estimates, not quotes.

| | Cloudflare Workers + DO | AWS Lambda + DynamoDB + APIGW WS | Fly.io, one Rust binary + SQLite |
|---|---|---|---|
| **1 user** | $5 (Paid plan minimum) | ~$0–1 (free tiers; APIGW's lasts 12 months) | ~$0.50–2.50 with autostop, ~$2–3 always on |
| **100 users** | $5 | ~$1–3 | ~$2–3 |
| **1,000 users** | ~$6–15 (DO requests $0.15/M past 1M; duration depends on how long objects stay awake, to measure) | ~$12–15 (HTTP API ~$3, WS minutes ~$3.60, WS messages ~$3, DynamoDB ~$3, Lambda ~$0.50) | ~$3–5 (one 512 MB machine) |
| **Wake from idle** | isolate: ms; DO cold wake **unmeasured** | Rust init ~20–50 ms, plus API Gateway | ~2 s from stopped, or pay to stay on |
| **Per-user ordering** | free: one object per user is single-threaded | **build**: `TransactWriteItems` on a per-user head item (conditional on the last number) plus an idempotency item, retry on conflict | **build**, easy: per-user mutex in one process, SQLite single writer |
| **Push to devices** | free: hibernating WebSockets in the user's object | **build**: connection table, `PostToConnection` to each, clean up dead ones | **build**, easy: broadcast channel per user |
| **Durability** | managed: 30-day point-in-time recovery; we add a nightly export to R2 | managed: 35-day PITR + export to S3 | **ours**: Litestream to R2, restore drills, exactly one machine and volume |
| **Reminders** | DO alarms | EventBridge Scheduler | external trigger (a stopped machine runs nothing) |
| **One origin (iOS cookies, passkeys)** | yes, one Worker | CloudFront in front of S3 + API Gateway | yes, one binary |
| **MCP** | `rmcp` types + ~200 lines (done here) | full `rmcp`, both eras | full `rmcp`, both eras |
| **Services to run** | 1 (+ R2 for backups) | ~7: Lambda, API Gateway (HTTP + WS), DynamoDB, CloudFront, S3, EventBridge, IAM | 1 machine + object storage |

**Reading it.** At personal scale every option costs less than a coffee, so
performance per dollar doesn't decide anything; build and run effort does.
Fly is cheapest from about 100 users up, but it is a single box whose data
safety we own. Lambda matches the "tiny functions" idea, but the
single-writer actor and the push channel that a Durable Object gives for
free become our code across several services. A Durable Object is the
"tiny unit" idea taken further: one small isolate per *user*, started in
milliseconds, with its database inside it.

**Fallback choice.** Lambda over Fly, as the owner prefers: storage that is
managed and backed up beats a cheaper box for data that is the owner's
source of truth. Expect a few hundred more lines (ordering, fan-out) and
infrastructure-as-code for ~7 services.

## Next steps

1. **Deploy the prototype** to measure what local runs can't: Durable
   Object cold wake, Worker startup with this wasm, real CPU time per
   request, and Previews per PR. Needs a Cloudflare account (Paid plan,
   $5/month, for the Argon2id CPU budget) and an API token as a GitHub
   secret.
2. Build the OAuth authorization server and connect Claude to `/mcp`.
3. Upstream the `webauthn-rs-core` clock fix.

Sources: [API Gateway pricing](https://aws.amazon.com/api-gateway/pricing/),
[DynamoDB on-demand pricing](https://aws.amazon.com/dynamodb/pricing/on-demand),
[Durable Objects pricing](https://developers.cloudflare.com/durable-objects/platform/pricing),
[Fly.io pricing](https://fly.io/docs/about/pricing/); earlier research in
`2026-11-rust-server-hosting.md`.
