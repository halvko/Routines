# 0002: The core is one synchronous `update(event)`, fed through one queue

Accepted, 2026-11-06.

## Decision

- The core has **one entry point**: `update(event) -> (state, effects)`.
  Every source of change is an event: a tap, a timer, a message from the
  server, the result of a request the core asked for.
- `update` is **synchronous and never waits**. The core does no I/O; it
  returns effects ("save this", "send this", "keep me subscribed to sync"),
  and the shell carries them out. Their results come back later as new
  events.
- Each shell feeds the core through **one serial queue**. In the browser that
  is the event loop plus a guard: an event raised while `update` runs is
  queued and handled after it returns, never by calling `update` again. A
  native shell owns the core on one serial queue or actor and posts every
  call to it.
- **The shell owns connections.** WebSocket or polling, reconnects, backoff
  and auth live in the shell; the core only sees events like "server delta
  arrived".
- **Stale replies are the core's job.** Serialization orders events, not the
  world: a reply to an older request can arrive after a newer local change.
  Requests carry ids or versions, and the core ignores replies that no
  longer apply.

## Why

- One door in gives one order of events, one place for saving, undo and
  re-rendering, and tests that are lists of events.
- A synchronous, I/O-free `update` cannot interleave with itself, and is
  trivial to call across the WebAssembly boundary.

## Rules out

- Async functions in the core, callbacks from the shell into core internals,
  and separate entry points per event source.
