# 0005: The server runs on Cloudflare Workers, in Rust, on the Paid plan

Accepted, 2026-11-06, **to be confirmed by a prototype** (below).

## Decision

- **Cloudflare Workers, written in Rust with `workers-rs`**, on the Paid plan
  (about $5/month).
- **One SQLite-backed Durable Object per user** holds that user's data and is
  the single writer that numbers their sync log (0003). Its hibernating
  WebSockets push changes to the user's devices; its alarms will schedule
  reminders.
- Accounts and sessions live in the user's Durable Object, with a small
  directory object mapping email to user. No D1 unless it turns out simpler.
- **One origin** serves the web app, the API, sign-in and the MCP endpoint.
  Safari blocks cross-site cookies by default, so the web app moves from
  GitHub Pages to the Worker when the server arrives (see "Moving to the new
  address" below).
- **Backups:** Durable Object point-in-time recovery (30 days) plus a nightly
  export to R2.
- **Per-PR previews** with Worker Previews, each with its own Durable Objects.
- **Hand-rolled** in Rust: the OAuth authorization server Claude's connector
  needs (no Rust crate implements it on any host) and a small MCP JSON-RPC
  handler using `rmcp`'s protocol types (its HTTP transport needs Tokio,
  which Workers does not have). Passkeys with `webauthn-rs`, passwords with
  RustCrypto `argon2`.

## Moving to the new address

`localStorage` belongs to an origin, and the home-screen icon points at the
github.io address, whose service worker keeps serving the cached old app. So
the move is a step of its own, built before the old address is retired:

1. **The last github.io release** (the frozen JavaScript app, one bug-fix
   release) adds a "Move to the new app" banner. The new app, once signed in,
   shows a one-time transfer code. Entering it in the old app uploads its
   `routines.v1` data to the account, over one import endpoint that accepts
   the github.io origin and nothing else. The import goes through the core and
   is idempotent (0003).
2. After a successful upload the old app **stops accepting changes** (so the
   two copies cannot drift), points to the new address, and explains how to
   add the new icon to the home screen and remove the old one. It also offers
   **"Keep using this app"**, which unfreezes it with its local data intact,
   for when the new app lets the owner down that morning. The move can then be
   retried. `routines.v1` is a bare snapshot (no per-field timestamps, no run
   events, hard deletes), so a second upload cannot simply be merged: the old
   app keeps the snapshot it uploaded the first time, and a later upload sends
   only the **difference against it**, turned into ordinary changes
   timestamped by the old device (something missing since then becomes a
   delete, which per 0003 is a status change). Where the new app changed the
   same thing since the first upload, timing does not decide: the conflict is
   shown to the user to resolve (0003).
3. **Export → Import stays available** as the fallback when the upload is not
   possible.
4. The github.io release stays up until every device has moved; only then is
   GitHub Pages switched off.

## The prototype that confirms it

One throwaway PR that proves, on the real runtime, what the research could
only show compiles:

1. A Rust Worker with a per-user Durable Object storing data in SQLite.
2. Passkey registration and sign-in with `webauthn-rs`, and one Argon2id hash
   within the CPU limit.
3. A WebSocket push from the Durable Object to a browser.
4. One MCP tool, behind the hand-rolled OAuth flow, that Claude can connect to
   and call.
5. Worker Previews working for a Rust project.

If any of these fights back for more than about a weekend's work, switch to
the runner-up instead of forcing it.

## Runner-up

A single Rust binary (axum, tokio, the full `rmcp`) on Fly.io with SQLite on a
volume and Litestream backups to object storage, about $1–3/month. Ordinary
Rust, but a ~2 s wake from stopped, our own backups, and an external scheduler
for reminders. Lambda with DynamoDB was ruled out: too many services for one
user.

Research: `docs/research/2026-11-rust-server-hosting.md`.
