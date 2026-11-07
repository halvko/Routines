# Where should the Rust server run?

Research date: 2026-11-06. Same conventions as `2026-11-stack-sync-auth.md`: inline primary sources, and **unverified** marks anything I could not confirm from one.

**Two kinds of evidence I produced myself:**
- **"My compile test"**: I built a crate for `wasm32-unknown-unknown` with Rust 1.97 in this session. This proves the crate builds, not that it runs on Workers.
- **"My measurement"**: timings taken in Node 22 / native code on a 2.1 GHz Xeon VM. Treat them as estimates.

**How I weighed the options.** Per the owner's note, alpha and pre-1.0 technology is acceptable when failure only means rework. Durability of the owner's data (backups, point-in-time recovery, export) still counts as a hard criterion.

## Executive summary

1. **Recommended: (A) Cloudflare Workers in Rust (workers-rs), Paid plan, $5/month.** Confidence: medium.
   - Durable Objects fit the chosen design almost exactly: one SQLite-backed Durable Object per user gives a single-writer actor, hibernating WebSockets for realtime, alarms for scheduled pushes, 30-day point-in-time recovery, and isolated per-PR preview namespaces.
   - The PWA, API, auth and MCP endpoint share one origin, which matters for cookies and passkeys on iOS.
2. **Close second: (B) one axum binary on Fly.io, with SQLite on a volume and Litestream to R2.** It is the most ordinary Rust (tokio, full `rmcp`, native crates) and costs roughly $1–3/month.
   - You pay with a cold start of about 2 s from stopped (a few hundred ms from suspended).
   - You must build scheduled wake-ups for reminders yourself.
   - You own backups and previews.
   Choose B if wasm32 friction on Workers turns out to be annoying.
3. **Not recommended: (C) Lambda + DynamoDB + API Gateway WebSockets.** It has the best Rust maturity and the fastest cold starts (about 20 ms init). But ordering, realtime fan-out and same-origin hosting each need extra AWS services, which is the highest operations burden for one user.
4. **OAuth for Claude: hand-roll it in every option.** No Rust crate implements the authorization-server subset Claude needs. `oxide-auth` has PKCE and refresh tokens but no DCR, CIMD or RFC 9728 support, and has had no release since 2024. The subset is about 8 endpoints and behaviours, which I estimate at 600–1,000 lines.
5. **MCP on Workers: use rmcp's protocol types, not its HTTP transport.** `rmcp`'s server types and its HTTP transport both *compile* to wasm32 (my compile test), but the transport calls `tokio::spawn`, which needs a tokio runtime that Workers doesn't provide. Use `rmcp::model` (or `tower-mcp-types`) with a small hand-written JSON-RPC handler, or try `pmcp`, which claims Workers support. On Fly or Lambda, use `rmcp`'s transport directly; it supports both protocol eras.
6. **Passkeys and Argon2id.** `webauthn-rs` 0.6.2 moved from OpenSSL to RustCrypto, and it compiles to wasm32 together with `argon2` 0.6 and `worker` 0.8.7 (my compile test). RustCrypto Argon2id with m=19 MiB, t=2 took about 37 ms as WASM and about 20 ms native (my measurement). That fits Workers Paid, Fly, and Lambda at 512 MB or more.

---

## Options compared

| | (A) Workers + workers-rs + DO | (B) Fly.io axum + SQLite + Litestream | (C) Lambda + DynamoDB + APIGW WS |
|---|---|---|---|
| OAuth AS (hand-rolled) | yes; state in a DO; CIMD fetches need SSRF care | yes; easiest (plain tokio/axum) | yes; state in DynamoDB |
| MCP | rmcp **types** + own JSON-RPC handler (transport needs tokio runtime) or `pmcp` (claims Workers support, unverified) | full `rmcp` Streamable HTTP, both eras | full `rmcp` via `lambda_http` (unverified glue) |
| Passkeys / Argon2id | webauthn-rs + argon2 compile to wasm32; ~37 ms/hash (Paid plan required) | native, ~20 ms | native; CPU scales with memory |
| Per-user ordering | DO is single-threaded per user | one process + SQLite single writer | DynamoDB conditional writes / `TransactWriteItems` on a per-user head item |
| Realtime | DO hibernating WebSockets | tokio broadcast + axum WS; keeps machine awake | APIGW WebSocket + connection table + `PostToConnection` |
| Scheduled pushes | DO alarms | needs external wake-up (unverified options) | EventBridge Scheduler (not researched) |
| Wake-up when idle | isolate start; **DO cold wake unverified** (one third-party test: ~1.5 s) | ~2 s from stopped, a few hundred ms from suspended | Rust init ~20 ms + APIGW |
| Backups / PITR | DO PITR 30 days + own nightly export to R2 | Fly snapshots daily (5–60 days) + Litestream continuous | DynamoDB PITR up to 35 days + export |
| Per-PR previews | Worker Previews with isolated DO namespace | `fly-pr-review-apps` action | DIY stacks |
| Cost/month (1 user) | $5 | ~$1–3 | ~$0–1 (after 12-month API Gateway free tier: cents) |

---

## (A) Cloudflare Workers in Rust with Durable Objects

**Status of workers-rs** (`worker` 0.8.7, 2026-09-25, [crates.io](https://crates.io/crates/worker)).
- The README warns of "a few rough edges, some unimplemented APIs". D1 is alpha, Queues are beta and RPC is experimental.
- There is "no Tokio or async_std support". Panics abort the instance unless you build with `--panic-unwind`.
- On bundle size: "minimize the number of crates". ([workers-rs README](https://github.com/cloudflare/workers-rs))
- In the 0.8.7 source, `durable.rs` exposes `accept_web_socket`, `accept_websocket_with_tags`, `get_websockets`, `set_alarm`, `sql()` and `websocket_message`, so the hibernation, alarm and SQLite APIs are wrapped ([docs.rs State](https://docs.rs/worker/latest/worker/durable/struct.State.html)).
- **Not wrapped:** the DO point-in-time-recovery calls (`getBookmarkForTime`, `onNextSessionRestoreBookmark`). I grepped the source and found no "bookmark" outside D1. You would call them through `wasm-bindgen` JS interop. That is a small fix, and you need it only for restores.

**Limits** ([Workers limits](https://developers.cloudflare.com/workers/platform/limits/), [DO limits](https://developers.cloudflare.com/durable-objects/platform/limits/)).
- Paid plan: 30 s CPU per request by default. Free plan: 10 ms per request.
- 128 MB memory per isolate. Worker startup must finish within 1 s.
- DOs get 30 s CPU, reset by each request or WebSocket message.
- Cloudflare warns that unoptimised Rust WASM "may exceed Worker bundle size limits or experience long startup times" ([Rust on Workers](https://developers.cloudflare.com/workers/languages/rust/)).

**1. OAuth AS.** Possible, with all state kept in a DO, as the owner wants (see "Hand-rolled OAuth subset" below for the endpoints).
- CIMD requires your server to fetch a client-supplied URL. Cloudflare's own provider enables `global_fetch_strictly_public` for that ([workers-oauth-provider](https://github.com/cloudflare/workers-oauth-provider)), and the same flag protects a Rust Worker because it applies to the runtime's `fetch`.
- **Escape hatch if this proves painful:** a thin TypeScript entry Worker running `@cloudflare/workers-oauth-provider` in front of the Rust DOs. It needs a KV namespace. This only means rework; no data is lost.

**2. MCP.**
- *My compile test:* `rmcp` 3.9.0 compiles to wasm32 with `default-features = false`, both with `server` alone and with `transport-streamable-http-server`. It needs `uuid/js` and the `getrandom` `wasm_js` backends.
- The streamable server, however, calls `tokio::spawn` (in `transport/streamable_http_server/tower.rs`). That needs a tokio runtime, and the Workers executor isn't one, so I expect a runtime panic (**unverified**, not run).
- Workable paths:
  - (i) Deserialize requests with `rmcp::model` types and dispatch `server/discover`, `initialize`, `tools/list` and `tools/call` yourself. This is a few hundred lines for a stateless JSON-only server (my estimate).
  - (ii) Use `tower-mcp-types` 0.25.1, described as "Standalone MCP protocol types -- no Tower/Tokio required, WASM-safe" ([crates.io](https://crates.io/crates/tower-mcp-types)).
  - (iii) Use `pmcp` 2.23.0. Its README claims "Cloudflare Workers (wasm32-unknown-unknown)" via `--features wasm`, and says it serves 2025-11-25 and 2026-07-28 "negotiated per request" ([pmcp README](https://github.com/paiml/rust-mcp-sdk), [crates.io](https://crates.io/crates/pmcp)). This is unverified, and pmcp is a smaller project.
- If the hand-written handler breaks, you rework the handler; no data is at risk.

**3. Passkeys and Argon2id.**
- `webauthn-rs` 0.6.2 (2026-10-31) states "As of v0.6.0, webauthn-rs uses RustCrypto … instead of OpenSSL". It "has passed a security audit performed by SUSE product security" ([README](https://github.com/kanidm/webauthn-rs), [crates.io](https://crates.io/crates/webauthn-rs)). I found no audit date, so which version was audited is unverified.
- *My compile test:* `webauthn-rs` 0.6.2, `argon2` 0.6.0 and `worker` 0.8.7 (with `d1`) build together for wasm32. Running passkey registration inside workerd is untested.
- *My measurement:* RustCrypto `argon2` compiled to wasm32 and run in Node 22 takes about 37 ms at m=19456, t=2 (OWASP's minimum) and about 26 ms at m=7168, t=5. That is fine on Paid (30 s CPU) and impossible on Free (10 ms).
- Free would work only if a DO really gets 30 s CPU on Free. That remains **unverified**, as noted in the first report.

**4. Realtime and ordering.**
- One DO per user is single-threaded, so it assigns `serverSeq` with no locking.
- Clients connect through a hibernating WebSocket, which costs no duration charges while idle ([DO WebSockets](https://developers.cloudflare.com/durable-objects/best-practices/websockets/)).
- Put accounts and sessions in the same per-user DO, plus a tiny "directory" DO that maps email to user id. Encode the user id in the session-token prefix so the Worker can route requests without a lookup. This routing is my design, not sourced.

**5. Wake-up latency.**
- Workers try to hide isolate cold starts. Cloudflare's 2025 post says complex apps started losing the TLS-handshake race and describes "shard and conquer" routing to warm isolates ([Cloudflare blog 2025](https://blog.cloudflare.com/eliminating-cold-starts-2-shard-and-conquer/), [2020 post](https://blog.cloudflare.com/eliminating-cold-starts-with-cloudflare-workers/)).
- I found no official figure for DO cold wakes. One third-party benchmark measured about 1,494 ms ([Lumenize](https://lumenize.com/blog/cloudflare-do-facets-in-practice)). That is **unverified** and a single workload, so measure it yourself.

**6. Operations.**
- Deploy with Wrangler from GitHub Actions. Secrets go in through `wrangler secret put` (not researched further).
- **Worker Previews** (Wrangler 4.135.0 or later) give each branch its own URL, secrets, and an *automatically provisioned DO namespace*. They are on Free and Paid, limited to 100 / 500 previews per Worker ([Previews](https://developers.cloudflare.com/workers/previews/)). That they work for workers-rs projects is unverified; they are a Wrangler feature, so they probably do.
- **Backups:** DO point-in-time recovery to "any point in time in the past 30 days" ([SQLite storage API](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/)). There is no documented export or dump, so add an alarm-driven nightly JSON export to R2.
- R2 Free: 10 GB-month storage, 1M Class A and 10M Class B operations, free egress ([R2 pricing](https://developers.cloudflare.com/r2/pricing/)).

**7. Cost.**
- $5/month account minimum for Workers Paid. It includes 10M requests, 30M CPU-ms and DO allowances (1M requests, 400k GB-s, 25B rows read, 50M rows written, 5 GB-month) ([pricing](https://developers.cloudflare.com/workers/platform/pricing/)).
- One user stays well inside that. Static assets are free. **Total ≈ $5.**

---

## (B) A single Rust binary (axum) on Fly.io

**Stack.** `axum` 0.8.9 ([crates.io](https://crates.io/crates/axum)), serving static PWA assets, the API, WebSockets, the OAuth endpoints and MCP. SQLite sits on a Fly volume, with Litestream replicating it to R2 or Tigris.

**1. OAuth AS.** Hand-roll it as below. The ecosystem is the same as for (A); there are simply no wasm constraints.

**2. MCP.** Full `rmcp`. Its Streamable HTTP server is a Tower service ([rust-sdk](https://github.com/modelcontextprotocol/rust-sdk)).
- `StreamableHttpServerConfig` has `legacy_session_mode`, documented as applying "only to legacy protocol versions (< 2026-07-28)". Requests that negotiate 2026-07-28 "are always served statelessly". Its `json_response` option prefers plain JSON responses. So one server answers both eras (from the rmcp 3.9.0 source).
- Set `allowed_hosts`. The default accepts loopback only.

**3. Passkeys and Argon2id.** `webauthn-rs` and `argon2` run natively. *My measurement:* about 20 ms per hash at m=19 MiB, t=2 on one core. A shared-cpu-1x machine with 256 MB fits a 19 MiB hash (the CPU share on Fly's shared machines is unverified).

**4. Realtime and ordering.**
- Run exactly one machine. SQLite's single writer, plus a per-user `tokio::sync::Mutex`, gives total order. A `tokio::sync::broadcast` channel per user fans changes out to WebSocket clients.
- **Do not run two machines with two volumes.** They would diverge.
- Whether `fly launch` creates two machines by default is unverified, so check after first deploy.

**5. Wake-up latency.**
- From stopped: "A cold start takes about 2 seconds or more for common apps." From suspended: "Resuming a suspended Machine takes a few hundred milliseconds." Suspend needs 2 GB of memory or less and no swap, and its snapshots can be lost on deploys or host migrations, which forces a cold start. ([autostop/autostart](https://docs.fly.io/launch/autostop-autostart), [suspend/resume](https://docs.fly.io/reference/suspend-resume))
- A small Rust binary probably starts faster than "common apps" (unverified).
- The proxy only stops a machine when the app is "idle for several minutes". Whether an open WebSocket counts as non-idle is not stated (unverified); concurrency `soft_limit` drives the capacity decision.
- **Scheduled reminders:** a stopped machine runs nothing, so pushes at fixed times need an external trigger, such as a GitHub Actions cron or Fly scheduled Machines (not researched, unverified).

**6. Operations.**
- **Deploy:** the GitHub Actions workflow runs `superfly/flyctl-actions/setup-flyctl` and `flyctl deploy --remote-only` with a `FLY_API_TOKEN` deploy token ([Fly CD guide](https://docs.fly.io/launch/continuous-deployment-with-github-actions)).
- **Per-PR previews:** the `superfly/fly-pr-review-apps` action deploys `pr-<n>-<owner>-<repo>.fly.dev` and updates it on each push ([review apps](https://docs.fly.io/blueprints/review-apps-guide)). Cleanup on close isn't described, and volumes aren't mentioned, so previews can use ephemeral SQLite.
- **Backups:**
  - Daily volume snapshots, kept 5 days by default and configurable from 1 to 60. They "are not a continuous backup" ([snapshots](https://docs.fly.io/volumes/snapshots)).
  - Litestream v0.5.x ("Actively maintained") streams changes continuously ([litestream.io](https://litestream.io/)). Its defaults are a 1 s `sync-interval` and 24 h snapshot interval and retention; S3-compatible targets are supported ([config](https://litestream.io/reference/config/)).
  - Point-in-time restore is possible within retention; the exact `restore -timestamp` syntax is unverified.
  - Litestream runs as a second process, using its exec mode or a supervisor.
- **Secrets:** `fly secrets set` (not researched further).

**7. Cost** ([Fly pricing](https://docs.fly.io/about/pricing)).
- There is no free tier; new accounts get a $1 trial credit.
- shared-cpu-1x with 256 MB costs $2.19 per 30 days if it runs continuously. Stopped machines pay only rootfs at $0.15/GB-month. Volumes cost $0.15/GB-month even when stopped.
- The first 10 GB of snapshots, the SSL certificates and shared IPv4 are free.
- With autostop, expect roughly **$0.50–$2.50/month** (my estimate). R2 backups fall in R2's free tier.

---

## (C) Rust on AWS Lambda with DynamoDB

**Status.** Rust on Lambda became generally available on 2025-11-14, covered by the Lambda SLA ([AWS What's New](https://aws.amazon.com/about-aws/whats-new/2025/11/aws-lambda-rust)). Current crates: `lambda_runtime` 1.5.0 and `lambda_http` 1.3.2 ([crates.io](https://crates.io/crates/lambda_http)), and `aws-sdk-dynamodb` 1.137.0 ([crates.io](https://crates.io/crates/aws-sdk-dynamodb)).

**1–2. OAuth and MCP.** Hand-rolled OAuth. Full `rmcp` should work behind `lambda_http`, since it is Tower-based, but I haven't tested that glue (unverified).
- HTTP comes in through Lambda function URLs (`https://<id>.lambda-url.<region>.on.aws`) with auth type `NONE` or `AWS_IAM` ([function URLs](https://docs.aws.amazon.com/lambda/latest/dg/urls-configuration.html)).
- A custom domain needs CloudFront in front (inferred, unverified). Same-origin hosting with the PWA also needs CloudFront with an S3 origin.

**3. Argon2id and passkeys.**
- CPU scales with memory: "At 1,769 MB, a function has the equivalent of one vCPU" ([Lambda memory](https://docs.aws.amazon.com/lambda/latest/dg/configuration-memory.html)). A ~20 ms full-core hash would take roughly 280 ms at 128 MB (my extrapolation), so use 512 MB to 1 GB for the login function.
- webauthn-rs runs natively.

**4. Ordering and realtime.**
- There is no actor. Keep a per-user "head" item with `seq`. Each push runs `TransactWriteItems`: a conditional update of `seq = :expected` plus a `Put` of the operation row.
- That gives atomicity, serializable isolation against other writes, and up to 100 actions per transaction. A `ClientRequestToken` gives idempotency for 10 minutes. Each item costs 2 write units. ([DynamoDB transactions](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/transaction-apis.html))
- On conflict, retry. With one user this is rare.
- For realtime, use API Gateway WebSockets with a connections table, and send with `PostToConnection` after each commit. Pricing: $1 per million messages and $0.25 per million connection-minutes; the free tier lasts 12 months ([API Gateway pricing](https://aws.amazon.com/api-gateway/pricing/)).
- API Gateway WebSocket connection limits (I believe a 2 h maximum and 10 min idle) are **unverified**.

**5. Cold starts.** Rust init averages 19–22 ms at 128 MB in the daily lambda-perf benchmark (data 2026-11-06), the fastest runtime measured ([lambda-perf](https://maxday.github.io/lambda-perf/)). INIT time has been billed since 2025-08-01 ([AWS blog](https://aws.amazon.com/blogs/compute/aws-lambda-standardizes-billing-for-init-phase/)).

**6. Operations.**
- Infrastructure as code (SAM, CDK or Terraform) is needed for Lambda, the function URL, DynamoDB, the WebSocket API, CloudFront and S3, plus IAM.
- Per-PR previews mean a stack per PR, which you build yourself.
- **Backups:** DynamoDB PITR gives "up to 35 days of recovery points at a per second granularity" and restores into a new table ([PITR](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/Point-in-time-recovery.html)).

**7. Cost** ([Lambda](https://aws.amazon.com/lambda/pricing/), [DynamoDB](https://aws.amazon.com/dynamodb/pricing/on-demand/), [AWS Free Tier](https://aws.amazon.com/free/)).
- Lambda has 1M requests and 400k GB-s free per month. DynamoDB's always-free tier is 25 GB plus 25 provisioned WCU/RCU.
- New-account credits expire after 6 months on the free plan.
- At one user: about **$0–1/month**. PITR adds a small per-GB charge (price unverified).

---

## Other contenders I looked at

- **Google Cloud Run.**
  - Free tier with request-based billing: 180,000 vCPU-seconds, 360,000 GiB-seconds and 2 million requests per month ([Cloud Run pricing](https://cloud.google.com/run/pricing)).
  - Containers scale to zero. But there is no durable local disk for SQLite, so you'd add an external database such as Turso or Firestore, plus max-instances=1 for ordering. That makes it as complex as (C).
  - Cold-start figures and WebSocket limits are unverified.
- **Shuttle (shuttle.dev).** Shutting down, with a deadline around January 2026 per secondary sources ([shutdown docs index](https://docs.shuttle.dev/llms.txt)). Not a contender.
- **Hybrid: a TypeScript edge in front of Rust Durable Objects on Cloudflare.** If hand-rolled OAuth or MCP on wasm32 becomes a time sink, put Cloudflare's maintained `workers-oauth-provider` and `createMcpHandler` in a thin TypeScript entry Worker and keep all domain logic and storage in Rust DOs. This is a fallback for (A), not a separate platform.

## Hand-rolled OAuth subset Claude needs (any option)

Requirements are from [Claude's authentication docs](https://claude.com/docs/connectors/building/authentication) and the [MCP 2026-07-28 authorization spec](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization); see the first report for details.

1. **Protected resource metadata.** Serve `GET /.well-known/oauth-protected-resource` (RFC 9728). `resource` must equal the MCP URL exactly, and `authorization_servers` lists your issuer **first**.
2. **Authorization server metadata.** Serve `GET /.well-known/oauth-authorization-server` (RFC 8414) with:
   - `issuer`, `authorization_endpoint`, `token_endpoint`, `registration_endpoint`;
   - `response_types_supported: ["code"]` and `grant_types_supported: ["authorization_code","refresh_token"]`;
   - `code_challenge_methods_supported: ["S256"]` and `token_endpoint_auth_methods_supported: ["none"]`;
   - `client_id_metadata_document_supported: true` and `authorization_response_iss_parameter_supported: true`;
   - `scopes_supported`, including `offline_access`.
3. **DCR** (RFC 7591). `POST /register` takes JSON. Allowlist the redirect URIs `https://claude.ai/api/mcp/auth_callback` and loopback `http://localhost/callback` and `http://127.0.0.1/callback`, matched with any port.
4. **CIMD.** When `client_id` is an HTTPS URL, fetch it with SSRF protection. Check that the document's `client_id` equals the URL and that `redirect_uri` is listed in it. Cache the result.
5. **`GET /authorize`.**
   - Validate the client, the exact redirect URI, `response_type=code`, an S256 `code_challenge`, and `resource` equal to the canonical MCP URL.
   - Require a logged-in session (passkey or password) and show a consent screen.
   - Issue a single-use code (short-lived; my suggestion is 60 s) bound to the client, redirect URI, challenge, resource and scope.
   - Redirect with `code`, `state` and `iss`.
6. **`POST /token`** (`application/x-www-form-urlencoded`).
   - `authorization_code`: verify `BASE64URL(SHA256(verifier))` against the challenge and match the redirect URI.
   - `refresh_token`: **rotate** on every use, and revoke the whole family if an old token is reused.
   - Return `invalid_grant` on failure. Respond within 10 s (30 s for refresh).
7. **MCP endpoint.** Answer unauthenticated requests with `401` and `WWW-Authenticate: Bearer resource_metadata="…"`. Validate opaque access tokens: look up their hash, check expiry, and check the audience equals the resource.
8. **Optional:** token revocation, and a page listing connected clients.

**Crates.**
- `oxide-auth` 0.6.1 (June 2024) has PKCE and refresh-token primitives and adapters for actix, rocket, iron and rouille. `oxide-auth-axum` 0.6.0 dates from January 2025. I found no DCR, RFC 9728 or CIMD support in its 0.6.1 source. ([crates.io](https://crates.io/crates/oxide-auth))
- `oauth2` 5.0.0 and `openidconnect` 4.1.0 are *client* libraries ([crates.io](https://crates.io/crates/oauth2)).
- `rust-mcp-sdk` 2.0.0 offers "OAuth Authentication for MCP Servers" through a *remote* provider, an OAuthProxy and an `auth` feature ([README](https://github.com/rust-mcp-stack/rust-mcp-sdk)). That means a resource server delegating to an external authorization server, not an AS itself (my reading).
- **Building blocks:** `sha2`, `base64`, `getrandom`, `subtle`, `serde`, `url`.
- **Verdict:** hand-rolling is realistic; I estimate 600–1,000 lines plus tests. Validate it with the MCP Inspector and against Claude itself.

## Recommendation

**(A) Workers + workers-rs on the Paid plan. Confidence: medium.**
- **Why:** it fits the chosen design best:
  - per-user ordered SQLite and hibernating WebSockets come built in;
  - DO alarms can schedule reminder pushes;
  - point-in-time recovery comes built in, plus your own R2 export;
  - per-branch previews get isolated DO namespaces;
  - a single origin serves the PWA, cookies, passkey RP ID and MCP;
  - it costs $5.
- **Same-origin matters on iOS:** Safari blocks cross-site cookies by default ([WebKit 2020](https://webkit.org/blog/10218/full-third-party-cookie-blocking-and-more/)). The current GitHub Pages hosting would need the API on the same site, which (A) gives for free.
- **What may break, and the fix:**
  - The rmcp transport on wasm32 → write a JSON-RPC handler, or switch to `pmcp`.
  - Missing PITR bindings → JS interop.
  - WASM size or startup → trim crates and run `wasm-opt`.
  - Worst case → the TypeScript edge fallback.
  - None of these risk data loss.
- **When to choose (B) instead:** if fighting wasm32 costs more than about a weekend. It is the most normal Rust, but you take on a ~2 s cold wake (or suspend mode), external scheduling for pushes, and Litestream to run.

## Claims I'm least sure of

1. That `rmcp`'s HTTP transport panics on Workers. I inferred this from `tokio::spawn` and did not run it. Likewise, `pmcp`'s Workers support is only its README's claim.
2. Durable Object cold-wake latency. The only figure is one third-party benchmark (~1.5 s).
3. That `webauthn-rs` passkey ceremonies actually *run* in workerd. I only verified that it compiles.
4. Whether Worker Previews work unchanged for workers-rs projects.
5. Fly: whether open WebSockets block autostop, how fast a Rust binary cold-starts compared with "about 2 seconds" for common apps, and the best way to schedule wake-ups.
6. API Gateway WebSocket connection-duration and idle limits, and the DynamoDB PITR price.
7. That Durable Objects on the Free plan get 30 s CPU, carried over from the first report. It doesn't affect the $5 recommendation.
