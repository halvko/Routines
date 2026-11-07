# Routines: stack, sync, MCP and auth research

Researched 2026-11-06. Prices, limits and versions are as seen on that date unless noted. "Unverified" means I could not confirm it from a primary source. "My measurement" means I ran it myself in this session (Node 22 on a 2.1 GHz Xeon VM, 4 vCPU). These are rough estimates, not numbers taken on Workers.

## Executive summary (recommended stack, confidence)

1. **Hosting: Cloudflare Workers on the Paid plan ($5/month)** (high). One Worker serves the PWA as static assets, the sync API, the auth endpoints and the MCP endpoint. Each user gets one SQLite-backed Durable Object that holds the data and sends realtime updates over hibernating WebSockets. Worker Previews give you per-branch environments.
2. **Why Paid and not Free** (high). On Free, a Worker gets 10 ms of CPU per request. Argon2id with OWASP's parameters took 40–70 ms in WASM in my measurement. Even PBKDF2 at Workers' hard cap of 100k iterations took about 20 ms. Free works only if you skip server-side password hashing (passkeys and/or OPAQUE) or move hashing into a Durable Object. The DO option is unverified (see Q5).
3. **Sync: hand-rolled** (medium-high). Clients keep a queue of pending operations. Each user's Durable Object gives every operation a server sequence number. Clients pull with a cursor and rebase their pending operations on top. Templates and backlog fields merge last-write-wins per field, and run state is an append-only event log. Skip CRDT libraries and hosted sync engines. Zero has no offline writes, InstantDB is shutting down, and PowerSync and Electric need Postgres plus an always-on service.
4. **Shared core: TypeScript** (medium). The core is a set of pure reducers that run unchanged in React, in the Worker, in the MCP tools and later in React Native. Given your Rust preference, a Rust core is viable but costs more: Crux is pre-1.0, workers-rs calls itself "rough edges" with D1 in alpha, and every call crosses a WASM serialization boundary.
5. **MCP: in the same Worker** (high). Use `createMcpHandler` (stateless Streamable HTTP), with `@cloudflare/workers-oauth-provider` as the OAuth 2.1 authorization server and your own login page. Support CIMD and DCR, serve RFC 9728 metadata, require S256 PKCE, and allow the redirect `https://claude.ai/api/mcp/auth_callback`.
6. **Auth: passkeys first, password as fallback** (medium-high). Passkeys use SimpleWebAuthn. Sessions are opaque random tokens in a `__Host-` HttpOnly cookie, stored hashed in the database, with no JWTs on the first-party side. Passwords use Argon2id (19 MiB, t=2), a HIBP k-anonymity check, and per-account throttling. Hand-rolling this is reasonable for one user, and Better Auth is the fallback if multi-user or OAuth sign-in becomes real.
7. **Client-side hashing** (medium). Naive client hashing just makes the hash the password. "Server relief" works but is non-standard and enables user enumeration unless done carefully. OPAQUE (RFC 9807, Informational, July 2025) is the principled version. It is cheap on the server (about 1.4 ms in my measurement), which is its only real advantage here over passkeys.
8. **iOS push** (high). Web Push works only for Home Screen web apps, on iOS 16.4 and later. Declarative Web Push arrived with 18.4. Since iOS 26, any site added to the Home Screen opens as a web app by default.
9. **Biggest open risk** (medium). The MCP spec moved to a stateless 2026-07-28 revision. Claude documents support for the 2025-03-26, 2025-06-18 and 2025-11-25 *authorization* specs, so your server must keep serving older-era clients. Test against Claude itself.
10. **Cost at single-user scale.** About $5/month on Cloudflare Paid, or $0 on Free if you accept the password-hashing workaround. Only one custom connector is allowed on a Free Claude plan.

---

## 1. Hosting and backend platforms that scale to zero

### Cloudflare
- **Workers CPU and memory.** Free allows 10 ms CPU per HTTP request. Paid defaults to 30 s and can go up to 5 min. Memory is 128 MB per isolate on both plans. Free allows 100,000 requests/day. Free gets 50 subrequests per invocation, Paid 10,000. ([Workers limits](https://developers.cloudflare.com/workers/platform/limits/))
- **Worker size.** The limits page currently says 64 MiB uncompressed on both plans, with "no compressed size limit". That is much larger than the older 3 MB / 10 MB limits I remember, so re-check before relying on it. ([Workers limits](https://developers.cloudflare.com/workers/platform/limits/))
- **Pricing.**
  - Paid has a $5/month account minimum and includes 10M requests and 30M CPU-ms per month.
  - Durable Objects are on the Free plan, but only with the SQLite backend. Free DO allowances per day: 100k requests, 13,000 GB-s, 5M rows read, 100k rows written, and 5 GB stored.
  - D1 on Free: 5M rows read/day, 100k rows written/day, 5 GB.
  - KV on Free: 100k reads/day and only 1,000 writes/day.
  - DO SQLite storage billing had a target start date of 2026-01-07.
  ([Workers pricing](https://developers.cloudflare.com/workers/platform/pricing/))
- **Durable Objects limits.**
  - SQLite storage is 10 GB per object, but the same page's FAQ says 1 GB on Free. The page contradicts itself.
  - CPU is 30 s per request, and each incoming request or WebSocket message resets the timer.
  - Each object has a soft limit of 1,000 req/s.
  ([DO limits](https://developers.cloudflare.com/durable-objects/platform/limits/))
- **D1 limits.** Free: 500 MB per database and 10 databases. Paid: 10 GB per database. Time Travel keeps 7 days on Free and 30 on Paid. ([D1 limits](https://developers.cloudflare.com/d1/platform/limits/))
- **Realtime.** The DO WebSocket Hibernation API lets an object sleep while clients stay connected, with no duration charges while it hibernates. Attachments are capped at 16 KiB, and a forced code update disconnects sockets. ([DO WebSockets](https://developers.cloudflare.com/durable-objects/best-practices/websockets/))
- **Static assets and Pages.**
  - Static asset requests are "free and unlimited". ([billing](https://developers.cloudflare.com/workers/static-assets/billing-and-limitations/))
  - Cloudflare's migration guide puts frontend and API in one Worker. Assets are served *before* the Worker by default, and `run_worker_first` changes that. ([migrate from Pages](https://developers.cloudflare.com/workers/static-assets/migrate-from-pages/))
  - Third-party guides say Workers is now the recommendation for new projects. That is unverified from a Cloudflare page.
- **Per-branch previews.** Two features overlap here, and sources could be confused:
  - **Version URLs** (the old "preview URLs") are *not generated for Workers that implement a Durable Object*. ([Version URLs](https://developers.cloudflare.com/workers/configuration/previews/))
  - **Worker Previews** (`wrangler preview`, Wrangler 4.135.0 or later) are the newer per-branch feature. Each preview gets its *own auto-provisioned DO namespace*. D1 and KV are isolated only if you bind a separate resource. Previews are on the Free plan, with 100 per Worker on Free and 500 on Paid, and are public by default. ([Worker Previews](https://developers.cloudflare.com/workers/previews/), page last updated 2026-09-24)
- **Cold starts.** Workers start isolates within a 1 s startup budget ([limits](https://developers.cloudflare.com/workers/platform/limits/)). I found no current primary figure for typical cold-start latency (unverified).
- **Lock-in.** High for Durable Objects: there is no equivalent elsewhere, so the per-user actor pattern would need re-hosting. Plain SQLite schemas and a fetch-style handler port fine.
- **Fit for one user on many devices.** Excellent. A per-user Durable Object is a natural single-writer sequencer with built-in WebSocket fan-out.

### AWS (Lambda, DynamoDB, API Gateway, Cognito)
- **Lambda.**
  - The free tier is 1M requests and 400,000 GB-s per month. Memory ranges from 128 MB to 10,240 MB. ([Lambda pricing](https://aws.amazon.com/lambda/pricing/))
  - Since 2025-08-01, the INIT (cold-start) phase is billed for ZIP-packaged managed runtimes. ([AWS blog](https://aws.amazon.com/blogs/compute/aws-lambda-standardizes-billing-for-init-phase/))
- **Free tier structure.** New accounts get a "free plan" with $100–$200 in credits. It closes after 6 months or when the credits run out. Some services are "always free", but the page doesn't list them. ([AWS Free Tier](https://aws.amazon.com/free/))
- **DynamoDB.** Always-free covers 25 GB plus 25 WCU and 25 RCU, *provisioned* capacity only. On-demand costs $0.625 per million writes and $0.125 per million reads. ([DynamoDB pricing](https://aws.amazon.com/dynamodb/pricing/on-demand/))
- **API Gateway WebSockets.** The free tier is 1M messages and 750k connection-minutes, *for 12 months only*. After that it's $1.00 per million messages and $0.25 per million connection-minutes, metered in 32 KB increments. ([API Gateway pricing](https://aws.amazon.com/api-gateway/pricing/))
- **Cognito.** Lite gives 10k MAU free but no passkeys. Essentials gives 10k MAU free with passkeys, then $0.015 per MAU. ([Cognito pricing](https://aws.amazon.com/cognito/pricing/))
- **Cold starts** (daily lambda-perf benchmark, data generated 2026-11-06, hello-world at 128 MB, ZIP, averaged by me from the raw data):

  | Runtime | Average init |
  |---|---|
  | Rust (provided.al2023) | 21.5 ms (x86), 19.3 ms (arm64) |
  | Go | 48–58 ms |
  | Python 3.13 | 77–87 ms |
  | Node 22/24 | 120–143 ms |

  ([lambda-perf](https://maxday.github.io/lambda-perf/), [raw data](https://github.com/maxday/lambda-perf))
- **Fit.** It works, but it's more moving parts: IAM, API Gateway, DynamoDB conditional writes for ordering, and a connection table for WebSockets. There is no single-threaded per-user coordinator like a Durable Object. Lock-in is high, spread across several services.

### Supabase
- **Free plan.** 500 MB of database, 50k auth MAU, 500k Edge Function invocations, Realtime with 200 concurrent connections and 2M messages, and 2 projects. **Projects pause after 1 week of inactivity.** Pro costs $25/month. ([Supabase pricing](https://supabase.com/pricing))
- **Edge Functions.** 256 MB memory, 2 s CPU per request, and 150 s wall-clock on Free. ([limits](https://supabase.com/docs/guides/functions/limits))
- **Fit.** Postgres is portable, so lock-in is low to medium. It is the natural partner for PowerSync or Electric. Weak points: the pause, an always-on Postgres that doesn't truly scale to zero, and passkeys still being experimental (see Q6).

### Fly.io and Turso
- **Fly.io.** There is no free tier for new orgs, just a $1 trial credit for 7 days. The smallest machine (shared-cpu-1x, 256 MB) costs $2.19 per 30 days if it runs continuously. Autostop and autostart exist. Volumes cost $0.15/GB-month even while stopped. ([Fly pricing](https://docs.fly.io/about/pricing)) Fly suits a single Rust binary (axum plus SQLite on a volume), but you operate it yourself.
- **Turso** (hosted libSQL, a database only). Free: 100 databases, 5 GB, 500M rows read and 10M written per month, and 3 GB/month of syncs. The paid plan starts at $4.99/month. ([Turso pricing](https://turso.tech/pricing)) It would pair with Workers or Lambda, but Durable Object SQLite already covers this need.

### Comparison

| | CF Workers + DO | AWS Lambda + DynamoDB + APIGW | Supabase | Fly.io |
|---|---|---|---|---|
| $ at 1 user | $0 (Free) / $5 (Paid) | ~$0 at first (credits, 12-month APIGW tier), then cents | $0 (pauses after 7 days idle) / $25 | ~$2+ |
| CPU per request | 10 ms Free / 30 s+ Paid | up to the timeout; CPU scales with memory | 2 s | unlimited |
| Realtime | DO hibernating WebSockets | APIGW WebSockets | Realtime (Postgres) | DIY |
| Cold start | isolates (no primary figure) | Rust ~20 ms, Node ~130 ms | unverified | machine start (unverified) |
| Lock-in | high (DO) | high (multi-service) | low–medium | low |
| Ops burden | lowest | highest | low | medium |

**Recommendation.** Cloudflare Workers Paid, with one SQLite-backed Durable Object per user. Free is viable only with the auth caveats in Q5.

---

## 2. Sync for an offline-first single-user app

### Status of the engines (versions from npm/crates as of 2026-11-06)
- **Replicache.** In maintenance mode: open-sourced, free, with no new features, and users are told to move to Zero. ([replicache.dev](https://replicache.dev/)) Latest is 15.3.0, from 2025-07 ([npm](https://www.npmjs.com/package/replicache)). Its push/pull/rebase protocol is still the best documented model to copy.
- **Zero.** 1.0 became GA in March 2026 ([status](https://zero.rocicorp.dev/docs/status)), and 1.11.4 is current ([npm](https://www.npmjs.com/package/@rocicorp/zero)). It **"does not support offline writes"**: while disconnected, writes are rejected ([connection docs](https://zero.rocicorp.dev/docs/connection)). It also needs Postgres plus `zero-cache` ([llms.txt](https://zero.rocicorp.dev/llms.txt)). **Disqualified** for phone-offline edits.
- **PowerSync.**
  - Offline writes work: they go into a FIFO upload queue that your `uploadData()` sends to *your* API. ([client architecture](https://docs.powersync.com/architecture/client-architecture))
  - Source databases are Postgres, MongoDB, MySQL and SQL Server, so not D1.
  - The free cloud tier gets deactivated when idle. The page says 1 week in one place and 2 weeks in another, so it contradicts itself. Pro costs $49/month.
  ([pricing](https://www.powersync.com/pricing))
  - Active: @powersync/web 2.6.0 was released 2026-11-02 ([npm](https://www.npmjs.com/package/@powersync/web)).
- **Electric.** A "read-path sync engine for Postgres": you build writes through your own API ([docs](https://electric.ax/docs/intro)). Active, at 1.5.28 ([npm](https://www.npmjs.com/package/@electric-sql/client)).
- **Triplit.** Its co-founder joined Supabase on 2025-10-08 ([Supabase blog](https://supabase.com/blog/triplit-joins-supabase)). The last npm release was 2025-07-31 ([npm](https://www.npmjs.com/package/@triplit/client)). Treat it as stagnant.
- **InstantDB.** **Sunsetting.** New signups are closed and cloud apps shut down on 2027-08-31. The team joined OpenAI, and the code is open source and self-hostable. ([essay](https://www.instantdb.com/essays/instant_team_joins_openai)) **Avoid.**
- **Jazz.** The stable line is 0.20.x. v2 is a rewrite and still in alpha (2.0.0-alpha.59, 2026-10-04) ([npm](https://www.npmjs.com/package/jazz-tools), [jazz.tools](https://jazz.tools/)). Too much churn for now.
- **LiveStore.** Event-sourced SQLite on the client, with a Cloudflare Workers / Durable Object sync provider. It is pre-1.0 (0.5.0, 2026-10-15) and its docs are "still work in progress". ([docs](https://docs.livestore.dev/), [npm](https://www.npmjs.com/package/@livestore/livestore)) It is the closest off-the-shelf match to the design recommended below.
- **CRDT libraries.** All three are alive:
  - Automerge 3.5.0 (2026-09). 3.0 claimed more than 10x lower memory. ([blog](https://automerge.org/blog/))
  - Yjs 13.6.33 (2026-09) ([npm](https://www.npmjs.com/package/yjs)).
  - Loro 1.17.1 (2026-10) ([npm](https://www.npmjs.com/package/loro-crdt)).

### What is correct for this data
This part is my analysis, not a cited source.
- One user, a handful of devices, and the conflicts you'd actually see are "edited the same template on phone and laptop while offline".
- **Routine templates and backlog items** are records with scalar fields plus ordered lists. Per-field last-write-wins is fine here. Use server sequence numbers, or hybrid logical clocks, as the tiebreak. For ordering, use fractional indexes (string sort keys) instead of array positions.
- **Run state** ("step X done at T", flag choices) is naturally an **append-only event log**. Merging is a union, and state is a fold over the events. Only "undo" needs a design decision.
- So you don't need a CRDT library, which mainly pays off for collaborative rich text. The simplest correct design is Replicache-style:
  1. The client applies a mutation locally and appends `{id, name, args, clientSeq}` to a pending queue in IndexedDB.
  2. It pushes to the user's Durable Object. The DO runs **the same reducer** from the shared core, assigns `serverSeq`, stores the result, and broadcasts a poke over WebSocket.
  3. The client pulls changes since its cursor, replaces its base state, and replays the still-pending mutations on top (rebase).
  4. Make mutations idempotent by `(clientId, clientSeq)`.
- This gives deterministic convergence because one writer orders everything. The same mutation code powers the MCP tools. It's also easy for agents to write and for reviewers to read.
- **iOS storage caveat.** WebKit storage is "best-effort" and evicted least-recently-used. `navigator.storage.persist()` is more likely to be granted for Home Screen web apps, but not guaranteed ([WebKit storage policy](https://webkit.org/blog/14403/updates-to-storage-policy/)). Treat the server as the source of truth and the device as a cache plus outbox.

**Recommendation.** Hand-roll it as above, roughly a few hundred lines. If you'd rather adopt a library, consider LiveStore, knowing it is pre-1.0.

---

## 3. Remote MCP server and Claude

### What Claude requires (Anthropic docs)
- **Transport.** Use Streamable HTTP. Claude "also supports the legacy HTTP+SSE transport, which is being deprecated". ([Build an MCP server for Claude](https://claude.com/docs/connectors/building))
- **Auth spec versions.** Claude "follows the 2025-03-26, 2025-06-18, and 2025-11-25 authorization specifications". Supported auth types are `oauth_dcr`, `oauth_cimd` and `none` by default, with static headers in beta. ([Build](https://claude.com/docs/connectors/building), [Authentication](https://claude.com/docs/connectors/building/authentication))
- **Where Claude is stricter than the spec** ([Authentication](https://claude.com/docs/connectors/building/authentication)):
  - Sign-in starts only from a `401` with a `WWW-Authenticate: Bearer resource_metadata="…"` header. Claude ignores `WWW-Authenticate` on a 200.
  - The protected-resource metadata `resource` must exactly match the URL the user entered. Claude uses only the **first** entry in `authorization_servers`.
  - Claude uses CIMD only if the AS metadata has both `client_id_metadata_document_supported: true` and `"none"` in `token_endpoint_auth_methods_supported`. Otherwise it falls back to DCR.
  - PKCE is always S256. Advertise `code_challenge_methods_supported: ["S256"]`.
  - Register the redirect `https://claude.ai/api/mcp/auth_callback`. Also accept port-agnostic `http://localhost/callback` and `http://127.0.0.1/callback` for Claude Code.
  - The token endpoint must accept `application/x-www-form-urlencoded`. Return `invalid_grant` for dead refresh tokens, and rotate refresh tokens for public clients.
  - Claude allows 10 s for discovery, registration and token endpoints, and 30 s for refresh.
  - Traffic comes from `160.79.104.0/21`.
- **Limits.** Tool results can be about 150k characters, and tool calls time out at 240 s on claude.ai. Resource subscriptions and sampling aren't supported. ([Build](https://claude.com/docs/connectors/building))
- **Tool annotations.** Tools must declare `readOnlyHint` and `destructiveHint`. ([Claude MCP overview](https://claude.com/docs/connectors/building/mcp))
- **Plans.** Custom connectors work on Free, Pro, Max, Team and Enterprise, but "Free users are limited to one custom connector". ([Help Center](https://support.claude.com/en/articles/11175166-getting-started-with-custom-connectors-using-remote-mcp))

### MCP spec (latest revision 2026-07-28)
- **Stateless now.** The revision removes the `initialize` handshake and `Mcp-Session-Id`. It also removes the GET stream and SSE resumability, and adds `server/discover` and the `Mcp-Method`/`Mcp-Name` headers. ([changelog](https://modelcontextprotocol.io/specification/2026-07-28/changelog))
- **DCR is deprecated** in favour of Client ID Metadata Documents. ([changelog](https://modelcontextprotocol.io/specification/2026-07-28/changelog))
- **Authorization requirements.** Servers **MUST** implement RFC 9728 protected resource metadata. Clients MUST send RFC 8707 `resource`. Servers MUST validate the token audience. ([authorization](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization))
- **Backward compatibility.** Clients probe with a modern request and fall back to `initialize`. A server that wants to serve older clients must also implement the 2025-11-25 behaviour. ([Streamable HTTP, Backward Compatibility](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http))
- **Conflict to watch.** Anthropic's docs list only the 2025-era *authorization* specs. I couldn't confirm which *protocol* revision Claude's client speaks (unverified). Make sure your server accepts both initialize-era and stateless requests, and test against Claude.

### Libraries
- **MCP TypeScript SDK v2.** Released 2026-07-27 and split into `@modelcontextprotocol/server` and `@modelcontextprotocol/client` (2.4.0, 2026-10-23). It implements 2026-07-28. v1 (`@modelcontextprotocol/sdk` 1.32.2) gets fixes for at least 6 months after the v2 release. ([README](https://github.com/modelcontextprotocol/typescript-sdk), [npm](https://www.npmjs.com/package/@modelcontextprotocol/server)) The README names Node, Bun and Deno, not Workers. Workers support comes through Cloudflare's `agents` package.
- **Cloudflare `agents` (0.33.0, pre-1.0).**
  - `createMcpHandler` is the recommended stateless, Streamable-HTTP-only server. `McpAgent` (backed by a Durable Object) is **deprecated**, with a migration guide to MCP SDK v2.
  - `requiredScopes` is advertised but *not enforced*, so check scopes in your handlers.
  ([CF transport](https://developers.cloudflare.com/agents/model-context-protocol/protocol/transport/), [npm](https://www.npmjs.com/package/agents))
- **`@cloudflare/workers-oauth-provider` (1.2.4, 2026-10-22).**
  - Provides an OAuth 2.1 AS and resource server: PKCE, pre-registered clients, CIMD (via `clientIdMetadataDocumentEnabled`), DCR, RFC 9728, resource-bound tokens and step-up.
  - Tokens are stored hashed, and `props` are encrypted.
  - It needs a KV namespace `OAUTH_KV`, plus the `global_fetch_strictly_public` flag for CIMD.
  ([README](https://github.com/cloudflare/workers-oauth-provider))
  - Your own login goes in `/authorize`: `parseAuthRequest()`, then your UI, then `completeAuthorization()`. ([CF MCP authorization](https://developers.cloudflare.com/agents/model-context-protocol/protocol/authorization/))
  - The KV Free tier allows only 1,000 writes/day, which is plenty for one user ([pricing](https://developers.cloudflare.com/workers/platform/pricing/)).
- **Rust: `rmcp`** (official, 3.9.0, 2026-11-05).
  - It implements 2026-07-28 and stays compatible with 2025-11-25. The Streamable HTTP server is a Tower service (axum). It is tokio-based. ([rust-sdk](https://github.com/modelcontextprotocol/rust-sdk), [crates.io](https://crates.io/crates/rmcp))
  - Its OAuth support is **client-side** (discovery, CIMD/DCR, PKCE, refresh) ([OAUTH_SUPPORT.md](https://github.com/modelcontextprotocol/rust-sdk/blob/main/docs/OAUTH_SUPPORT.md)). You'd write or host the authorization server yourself and just validate bearer tokens in middleware.
  - I found no statement that rmcp runs on `wasm32` or Workers. workers-rs says there's "no Tokio … support" ([workers-rs](https://github.com/cloudflare/workers-rs)). So rmcp on Workers is unverified and likely needs work. Rust MCP is a better fit on Lambda or Fly.

**Recommendation.** Put the MCP endpoint in the TypeScript Worker using `createMcpHandler` and workers-oauth-provider. Enable both CIMD and DCR. Point `/authorize` at your existing session or passkey login, and have MCP tools call the user's Durable Object with the same core mutations.

---

## 4. Email and password best practice

### NIST SP 800-63B-4
- **Status.** **Final**, dated July 2025 ("07/31/25: SP 800-63B-4 (Final)"), and it supersedes the 2020 edition ([CSRC](https://csrc.nist.gov/pubs/sp/800/63/b/4/final)). The web version is stamped 2025-08-26 ([pages.nist.gov](https://pages.nist.gov/800-63-4/sp800-63b.html)). That's a minor date discrepancy.
- **Password rules** (§3.1.1, [SP 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b.html)):
  - At least **15 characters** for single-factor passwords, and at least **8** when used only within MFA.
  - SHOULD allow at least **64** characters, and SHOULD accept ASCII, space and Unicode. Each code point counts as one character, and NFC normalisation is recommended.
  - **SHALL NOT** impose composition rules or periodic rotation. A change SHALL be forced on evidence of compromise.
  - **Blocklist**: breached passwords, dictionary words and context words.
  - No hints and no security questions.
  - SHALL allow password managers and autofill, and SHOULD allow paste.
- **Throttling.** No more than **100** consecutive failed attempts (§3.2.2).
- **Storage.** Salted (at least 32 bits) and hashed with a cost factor "as high as practical". A separately-keyed hash (pepper) is SHOULD.
- **Sessions.**
  - Secrets of at least 64 bits. Cookies SHALL be Secure and SHOULD be HttpOnly.
  - Reauthentication at AAL1: SHOULD be at most 30 days overall.
  - At AAL2: at most 24 h overall and 1 h inactive.
  - **Email SHALL NOT be used for out-of-band authentication.**
- **Recovery** (§4.2). Emailed recovery codes are allowed, valid 24 h. Saved recovery codes need at least 64 bits and must be stored hashed.

### OWASP
- **Password Storage, in order of preference** ([cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)):

  | Algorithm | Parameters |
  |---|---|
  | Argon2id | m=19456 (19 MiB), t=2, p=1; equivalents 46 MiB/t1, 12 MiB/t3, 9 MiB/t4, 7 MiB/t5 |
  | scrypt | N=2^17 r=8 p=1 (128 MiB) … N=2^13 r=8 p=10 |
  | bcrypt | cost ≥10; 72-byte limit |
  | PBKDF2 | HMAC-SHA256 600,000; SHA512 220,000 |

  A pepper is optional defence in depth.
- **Sessions** ([Session Management](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)):
  - At least 64 bits of entropy, or 128 bits for a self-generated ID.
  - `__Host-` prefix with `Secure; HttpOnly; SameSite=Strict|Lax; Path=/`.
  - Regenerate the session ID on login.
  - **"Do not store authentication tokens, session IDs, JWTs, refresh tokens … in localStorage or sessionStorage."** Use HttpOnly cookies or a BFF instead.

  That supports server-side opaque session cookies over JWTs for a first-party app. The page doesn't compare JWTs and sessions directly.
- **Authentication** ([Authentication](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html)):
  - The same 8 / 15 length thresholds as NIST.
  - Generic errors, with consistent timing and status codes.
  - Throttle per account, not per IP, using exponential backoff.
  - Keep the reset flow reachable while locked out.
  - Re-authenticate before sensitive changes.
- **Forgot password** ([Forgot Password](https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html)):
  - Give the same response whether or not the account exists.
  - Tokens are random, single-use and expiring.
  - Never build the reset URL from the Host header. Use `Referrer-Policy: no-referrer`.
  - Don't log the user in automatically, and offer to revoke sessions.
  - Rate-limit per account.

### Breached-password check
The HIBP Pwned Passwords range API is `GET https://api.pwnedpasswords.com/range/{5 hex of SHA-1}`. It is **free, needs no API key, has no rate limit**, and requires no attribution. It uses k-anonymity: only a 5-character prefix leaves the server. Send `Add-Padding: true` to pad responses to 800–1,000 entries. ([HIBP API v3](https://haveibeenpwned.com/API/v3))

**Recommendation.**
- Minimum 15 characters, or 8 if the account also has a passkey. Maximum 128 or more.
- No composition rules. HIBP check plus a small context blocklist.
- Argon2id with 19 MiB and t=2.
- Per-account exponential backoff, with a hard limit well under 100.
- Opaque session token in a `__Host-` cookie, stored as SHA-256 in the database. 30-day absolute lifetime, and re-authenticate for credential changes.
- Reset by emailed single-use token (store a hash), valid 15–60 minutes. That window is my choice; NIST's 24 h is the upper bound for recovery codes.

---

## 5. Password hashing on Workers (and Lambda)

- **What's available.**
  - Web Crypto in Workers supports PBKDF2, HKDF, ECDH and X25519 for derivation, plus the non-standard `crypto.subtle.timingSafeEqual` ([Web Crypto](https://developers.cloudflare.com/workers/runtime-apis/web-crypto/)).
  - Under `nodejs_compat`, `node:crypto` is "fully supported" except `argon2`/`argon2Sync`. So `scrypt` and `pbkdf2` exist ([node:crypto](https://developers.cloudflare.com/workers/runtime-apis/nodejs/crypto/)).
- **Hard caps in the runtime source.** workerd's default limit enforcer caps **PBKDF2 at 100,000 iterations**, with the comment "WAY below the recommended minimum". It caps **scrypt at N·r·p ≤ 2^20**. ([limit-enforcer.h](https://github.com/cloudflare/workerd/blob/main/src/workerd/io/limit-enforcer.h)) That means OWASP's PBKDF2 setting of 600k is impossible. I'm assuming the production limits match the open-source defaults (unverified). The Cloudflare docs page doesn't state the cap.
- **scrypt.** OWASP's N=2^17, r=8 needs 128 MiB, which equals the whole isolate's memory, so it isn't practical. N=2^16, r=8, p=2 (64 MiB) fits the cost cap.
- **My measurement** (Node 22, the same algorithms; Workers CPU will be similar but not identical):

  | Algorithm | Time per hash |
  |---|---|
  | Argon2id WASM (hash-wasm) m=19 MiB t=2 | ≈70 ms |
  | Argon2id WASM m=7 MiB t=5 | ≈39 ms |
  | PBKDF2-SHA256 100k (Web Crypto) | ≈20 ms |
  | scrypt N=2^16 r=8 p=2 | ≈400 ms |

- **Free plan.** At 10 ms CPU per Worker request ([limits](https://developers.cloudflare.com/workers/platform/limits/)), *none* of the OWASP-grade options fit. Argon2id is 4–7x over and even capped PBKDF2 is about 2x over. Possible workarounds:
  - (a) **Hash inside a Durable Object.** The DO limits page lists 30 s CPU per request with no Free-plan qualifier ([DO limits](https://developers.cloudflare.com/durable-objects/platform/limits/)), and awaiting a DO from the Worker is I/O, not CPU. This is plausible but **unverified**. Test it, and expect Cloudflare to treat it as a loophole.
  - (b) Passkeys only.
  - (c) OPAQUE, where the server does about 1.4 ms of work (Q6b).
- **Paid plan.** 30 s CPU by default, so Argon2id at 19 MiB, t=2 in WASM (about 70 ms) is fine within 128 MB. Packaging WASM for Workers has had quirks, and hash-wasm's maintainer notes repackaging is needed ([discussion](https://github.com/Daninet/hash-wasm/discussions/56)). Alternatives are a small Rust `argon2` crate built to WASM ([crates.io](https://crates.io/crates/argon2), 0.6.0) or `argon2-wasm-edge`, which hasn't been updated since 2024 ([npm](https://www.npmjs.com/package/argon2-wasm-edge)).
- **AWS Lambda.** Native `@node-rs/argon2` (2.2.2, 2026-10, [npm](https://www.npmjs.com/package/@node-rs/argon2)) or the Rust `argon2` crate both work. Memory is configurable from 128 MB to 10 GB ([Lambda pricing](https://aws.amazon.com/lambda/pricing/)), and you pay about 70 ms per login, which is negligible.

**Recommendation.** Use Workers Paid with Argon2id in WASM. If you must stay on Free, make passkeys primary and either try workaround (a) or use OPAQUE.

---

## 6. Auth libraries (TypeScript, 2026)

| Option | Status (2026-11) | Workers + D1 | Passkeys | Custom hash | Sessions |
|---|---|---|---|---|---|
| **Better Auth** 1.7.18 (2026-11-05) | very active; **joined Vercel 2026-07-07** ("open source, framework and platform agnostic") ([blog](https://better-auth.com/blog/better-auth-joins-vercel)); took over Auth.js maintenance in Sept 2025 ([blog](https://www.better-auth.com/blog/authjs-joins-better-auth)) | runs with `nodejs_compat` ([Hono guide](https://www.better-auth.com/docs/integrations/hono)); D1 via Kysely community dialect or Drizzle ([adapters](https://www.better-auth.com/docs/adapters/other-relational-databases)); **no first-party D1 page found**; D1 has no transactions ([docs](https://www.better-auth.com/docs/authentication/email-password)) | `@better-auth/passkey`, built on SimpleWebAuthn, conditional UI, passkey-first sign-up ([docs](https://www.better-auth.com/docs/plugins/passkey)) | yes, `password.hash` / `password.verify`; default scrypt with unstated parameters ([docs](https://www.better-auth.com/docs/authentication/email-password)) | DB-backed cookie sessions, 7 days by default; optional signed cookie cache (revocation lags until it expires) ([docs](https://www.better-auth.com/docs/concepts/session-management)) |
| **Lucia** | **"deprecated in March 2025"**; the site now gives single-file reference code and "The Auth Book" ([lucia-auth.com](https://lucia-auth.com/)); npm `lucia` marked deprecated ([npm](https://www.npmjs.com/package/lucia)) | n/a | n/a | n/a | the reference code is DB sessions |
| **OpenAuth** 0.4.3 | site says "Beta"; no npm release since 2025-03 ([site](https://openauth.js.org/), [npm](https://www.npmjs.com/package/@openauthjs/openauth)), so it looks stalled | yes (KV) | unverified | unverified | issues OAuth tokens (JWT), the opposite of what you want |
| **Auth.js** (@auth/core 0.41.3) | maintenance mode under Better Auth ([GH discussion](https://github.com/nextauthjs/next-auth/discussions/13252)) | D1 adapter exists (community) | limited | n/a | n/a |
| **Clerk** | Hobby: 50k MRU, but **passkeys and MFA need Pro ($25/mo)** ([pricing](https://clerk.com/pricing)) | SDK | paid | no | managed |
| **Supabase Auth** | passkeys in **beta / experimental** since 2026-05-28 ([changelog](https://supabase.com/changelog/46458-passkeys-for-supabase-auth-beta), [docs](https://supabase.com/docs/guides/auth/passkeys)) | n/a | beta | no | JWT-based (unverified detail) |
| **Cognito** | Essentials: 10k MAU free, with passkeys ([pricing](https://aws.amazon.com/cognito/pricing/)) | n/a | yes | no | JWTs |
| **WorkOS AuthKit** | first 1M MAU free, including password, passkeys and MFA ([pricing](https://workos.com/pricing)) | SDK | yes | no | managed |

- **Passkey library.** SimpleWebAuthn server is at 14.0.3 (2026-09-25) and targets Node 22+ and Deno 2.4+ ([docs](https://simplewebauthn.dev/docs/packages/server), [npm](https://www.npmjs.com/package/@simplewebauthn/server)). Workers isn't listed. It runs there through Better Auth in practice, but that's unverified from the docs.
- **Oslo deprecated.** Older tutorials use `@oslojs/crypto` and `oslo`, which are now marked deprecated on npm ([npm](https://www.npmjs.com/package/@oslojs/crypto), [npm](https://www.npmjs.com/package/oslo)). Use Web Crypto directly.
- **Hand-rolling for one user is reasonable.** The Lucia author's current advice *is* to copy a single-file session implementation ([lucia-auth.com](https://lucia-auth.com/)). The surface is small:
  - Session table with cookie set/validate/revoke.
  - Passkey register/login with SimpleWebAuthn.
  - Argon2id hash/verify.
  - HIBP check, reset tokens, and a per-account throttle row.

  It's small enough to read in one review, and it needs no JWTs. The MCP side needs real OAuth tokens anyway, and workers-oauth-provider handles those.

**Recommendation.** Hand-roll as above for now. Move to Better Auth if you add other users or Google/Apple sign-in. Its Vercel ownership and community-only D1 adapter are mild risks.

### 6b. Client-side hashing, server relief, OPAQUE and passkeys (owner's question)
- **(a) Naive client-side hashing.** If the client sends `H(password)` and the server compares it, `H(password)` *is* the credential. Anyone who steals it or the database replays it directly, and TLS already protects it in transit. If the server stores it unhashed, a database leak equals full compromise. Fixing that means hashing it again on the server, which brings back the server cost. OWASP warns that pre-hashing schemes enable "password shucking" ([OWASP Password Storage](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)).
- **(b) Client-side key stretching ("server relief").** The client runs Argon2id and the server stores a *fast* keyed hash (HMAC or SHA-256) of the result. A leaked database still forces the attacker through Argon2id for each guess.
  - The term comes from the Catena paper, which defines a server-relief protocol "saving the server's resources at the cost of the client" ([Catena, ePrint 2013/525](https://eprint.iacr.org/2013/525)).
  - Real-world example: Bitwarden runs PBKDF2 at 600k on the client, and the server applies PBKDF2 again at 600k with a random salt ([Bitwarden whitepaper](https://bitwarden.com/help/bitwarden-security-white-paper/)).
  - Trade-off: the client needs its salt before login. Serving it lets anyone enumerate users unless unknown emails get deterministic fake salts, e.g. HMAC(secret, email). (My analysis.)
  - Trade-off: raising parameters requires a client update and re-wrapping at next login.
  - Trade-off: slow phones pay the cost.
  - Trade-off: the server ships the JS, so a compromised server can still capture plaintext.
  - Standards position: OWASP's Password Storage cheat sheet doesn't address client-side stretching at all. I checked the [source markdown](https://github.com/OWASP/CheatSheetSeries/blob/master/cheatsheets/Password_Storage_Cheat_Sheet.md) and it has no mention of "client" or server relief. NIST requires a salted, costly hash ([SP 800-63B-4 §3.1.1.2](https://pages.nist.gov/800-63-4/sp800-63b.html)). Server relief arguably meets the intent, but no standard explicitly blesses it (my reading).
- **(c) OPAQUE (augmented PAKE).**
  - **RFC 9807**, *Informational*, IRTF CFRG stream, July 2025. Not Standards Track. It hides the password from the server, including at registration, and resists precomputation if the server is compromised. ([RFC 9807](https://www.rfc-editor.org/rfc/rfc9807.html))
  - Rust `opaque-ke`: stable 4.0.1, with 4.1.0-pre.2 the latest pre-release ([crates.io](https://crates.io/crates/opaque-ke)). It is "based on RFC 9807". NCC Group audited v0.5.0 in June 2021 for WhatsApp, and the fixes landed in v1.2.0, so the current major version is unaudited. ([README](https://github.com/facebook/opaque-ke))
  - JS/WASM: **`@serenity-kit/opaque` 1.1.0** (2026-02) is built on opaque-ke ([README](https://github.com/serenity-kit/opaque), [npm](https://www.npmjs.com/package/@serenity-kit/opaque)). It uses an Argon2id KSF on the client; the default "memory-constrained" setting takes about 1 s on an M1. A 7ASecurity pentest and whitebox review exist ([opaque-auth.com](https://opaque-auth.com/)). React Native builds exist too.
  - `opaque-wasm` (2023) and `@cloudflare/opaque-ts` (2022) look stale ([npm](https://www.npmjs.com/package/opaque-wasm), [npm](https://www.npmjs.com/package/@cloudflare/opaque-ts)).
  - **My measurement** with `@serenity-kit/opaque` in Node: server `startLogin` ≈ **1.4 ms**, `finishLogin` ≈ 0.05 ms, and the client side with Argon2id at 19 MiB, t=2 ≈ 34–51 ms. So OPAQUE's server side fits Workers Free's 10 ms. I didn't test it inside workerd (unverified).
  - Costs: two round trips per login, a non-standard flow that no auth library here supports, and you own the protocol state handling.
- **Passkeys compared with all three, for a single user.** Passkeys are phishing-resistant and leave nothing secret on the server (only a public key). Server cost is one signature verification, and they sync through iCloud Keychain. NIST permits syncable authenticators below AAL3 ([SP 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b.html)). They dominate every password scheme for your threat model. A password matters only as a fallback for a new or lost device. For that fallback, a normal server-side Argon2id on Workers Paid is simpler than OPAQUE. OPAQUE earns its complexity only if you insist on Workers Free *and* a password, or if "the server never sees it" is a hard requirement.

---

## 7. Shared core: TypeScript or Rust to WASM

- **Crux.**
  - Pre-1.0 (crux_core 0.20.0, 2026-08-07) ([crates.io](https://crates.io/crates/crux_core)). Described as "production-ready" with "occasional breaking changes" expected.
  - Shells: SwiftUI, Compose, React/Vue, and Leptos/Yew on the web.
  - FFI now goes through **BoltFFI**, and the docs no longer say UniFFI. `facet-generate` produces Swift/Kotlin/TypeScript types, and messages are serialised with **Bincode**.
  - Users the README points to: Proton (Mail mobile) and PhotoRoom. ([README](https://github.com/redbadger/crux), [book](https://redbadger.github.io/crux/))
- **Costs of a Rust/WASM core in a React app.**
  - Every event and view model crosses a serialization boundary (Bincode in Crux).
  - You need a Rust toolchain plus `wasm-bindgen` and a bundler plugin.
  - Debugging across WASM means panic hooks and source maps.
  - The Rust-WASM working group's GitHub org was wound down in 2025, and wasm-bindgen moved to new maintainers ([Inside Rust, 2025-07-21](https://blog.rust-lang.org/inside-rust/2025/07/21/sunsetting-the-rustwasm-github-org)). It's still active, at 0.2.130 on 2026-10-30 ([crates.io](https://crates.io/crates/wasm-bindgen)).
  - I found no primary figure for bundle size. Expect hundreds of KB of WASM for a non-trivial core (unverified estimate).
- **Rust on the backend in 2026.**
  - **workers-rs** (`worker` 0.8.7, 2026-09-25):
    - The README warns of "a few rough edges, some unimplemented APIs". D1 is behind an **alpha** feature flag, Queues are beta, and RPC is experimental.
    - "No Tokio or async_std support". Panics abort the instance by default.
    - On bundle size: "minimize the number of crates". ([workers-rs](https://github.com/cloudflare/workers-rs), [crates.io](https://crates.io/crates/worker))
    - Durable Objects with SQLite storage are supported. `State` exposes the hibernation APIs `accept_web_socket`, `accept_websocket_with_tags` and `get_websockets` ([docs.rs](https://docs.rs/worker/latest/worker/durable/struct.State.html)).
    - CPU and memory limits are the same as for JS Workers ([limits](https://developers.cloudflare.com/workers/platform/limits/)). Cloudflare warns that unoptimised Rust WASM "may exceed Worker bundle size limits or experience long startup times" ([Rust on Workers](https://developers.cloudflare.com/workers/languages/rust/)).
  - **Rust on Lambda.** GA since 2025-11-14, covered by the Lambda SLA ([AWS What's New](https://aws.amazon.com/about-aws/whats-new/2025/11/aws-lambda-rust)). `lambda_runtime` is 1.5.0 ([crates.io](https://crates.io/crates/lambda_runtime)). Cold starts average about 19–22 ms at 128 MB, the fastest measured ([lambda-perf](https://maxday.github.io/lambda-perf/)). It's mature, but the platform weaknesses from Q1 still apply.
  - **Rust MCP.** rmcp is solid on Lambda or Fly (axum). On Workers it's unverified because of tokio (Q3).
- **Can the same core run in the backend?**
  - A **TypeScript** core runs unchanged in the Worker, the Durable Object, the MCP tools, React and React Native. That's the lowest-friction option.
  - A Rust core can run in a Rust Worker or DO (workers-rs) or be called from a TS Worker as a WASM module. You trade the immaturity listed above for one language.
- **Server push with a Crux-style core** (owner's question 3).
  - Crux has two built-in ways to talk to the shell: request/response effects and *stream* effects. `Command::stream_from_shell` returns a `StreamBuilder` that "builds a request expecting a (possibly infinite) sequence of responses from the shell (think WebSockets)" ([Crux effects](https://redbadger.github.io/crux/part-2/effects.html), [docs.rs Command](https://docs.rs/crux_core/latest/crux_core/command/struct.Command.html)). So the core can *ask* for a subscription, and the shell owns the socket and resolves the stream repeatedly.
  - The alternative is for the shell to own the WebSocket and feed each message in as an ordinary `Event`. That's simpler and also fine, and the same pattern works for a TS core (`dispatch(serverPoke)`).
  - My advice either way: keep connection lifecycle (reconnect, backoff, auth) in the shell, and give the core only "server delta arrived" events. I couldn't find a Crux doc page that compares the two (unverified as "recommended").

**Recommendation.**
- **TypeScript core** (medium confidence), written as pure reducers plus a mutation registry, with property tests. React, the Durable Object, MCP and React Native all import it.
- If the native app becomes concrete *and* you want Rust, consider Crux then. Build the core as a plain Rust crate with serde types now if you start in Rust. Don't adopt Crux before you have a native shell.
- A hybrid is also possible: a Rust Lambda or Fly backend with rmcp, and Cloudflare only for static hosting. It plays to your Rust strength but loses the Durable Object sync model and adds ops.

---

## 8. Web Push on iOS (installed PWAs)

- **Support.** Since **iOS/iPadOS 16.4**, but only for web apps **added to the Home Screen**. Permission must be requested from a direct user gesture. It uses the standard Push API, Notifications API and Service Workers, and no Apple Developer account is needed. Your server must allow `*.push.apple.com` endpoints. Badging works through `setAppBadge`, and notifications respect Focus modes. ([WebKit, 2023-02-16](https://webkit.org/blog/13878/web-push-for-web-apps-on-ios-and-ipados/))
- **Declarative Web Push** (WebKit post 2025-03-27, testable in iOS 18.4). The server sends JSON with `"web_push": 8030` and a `notification` containing a title and a navigate URL. It needs no service worker for display and gets no silent-push penalty. Browsers without support fall back to the classic path. ([WebKit](https://webkit.org/blog/16535/meet-declarative-web-push/)) The post doesn't explicitly say which release shipped it, so "shipped in 18.4" is unverified.
- **iOS 26 / Safari 26** (2025-09-15). "By default, every website added to the Home Screen opens as a web app," and there are "zero requirements for 'installability'" ([WebKit](https://webkit.org/blog/17333/webkit-features-in-safari-26-0/)). I found no 2025–2026 primary source that removes the Home Screen requirement for push. Secondary sources agree it still applies ([OneSignal docs](https://documentation.onesignal.com/docs/ios-web-push)).
- **EU.** In February 2024 Apple planned to remove Home Screen web apps in the EU in iOS 17.4 under the DMA, then reversed that before release ([MacRumors](https://www.macrumors.com/2024/02/15/ios-17-4-web-apps-removed-apple/), [9to5Mac](https://9to5mac.com/2024/03/01/apple-home-screen-web-apps-ios-17-eu/)). I couldn't find Apple's own statement on its current DMA page (unverified from a primary source).
- **Backend.** Sending VAPID Web Push from a Worker or a DO alarm works with Web Crypto. I didn't verify a specific library.

**Recommendation.** Add a "Get reminders" button that appears only when running standalone. Send declarative payloads plus a service-worker fallback, triggered from Durable Object alarms.

---

## Claims I'm least sure of
1. Whether a Durable Object on the **Free** plan really gets 30 s CPU, which would allow Argon2id on Free (Q5).
2. Which MCP *protocol* revision Claude's client speaks today. The docs only name auth-spec versions.
3. Whether production workerd uses the open-source PBKDF2 (100k) and scrypt (2^20) caps.
4. The 64 MiB Worker size limit on Free, which differs from older figures.
5. That `rmcp` can't run on Workers, and that SimpleWebAuthn and `@serenity-kit/opaque` run on Workers. Both are inferred, not tested.
6. Better Auth's default scrypt parameters, and that it has no first-party D1 adapter.
7. The EU status of Home Screen web apps, which rests on secondary sources.
