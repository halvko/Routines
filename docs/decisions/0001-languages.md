# 0001: Rust for the core and the server; TypeScript + React in the browser

Accepted, 2026-11-06.

## Decision

- **Core:** one Rust crate holding every domain rule (routines, runs, steps,
  options and bridging, placing, the backlog and its to-dos, carry-over, undo,
  migrations of stored data, sync merging). It does no I/O and has no UI.
- **Server:** Rust. It links the core natively, so the server applies synced
  changes with exactly the rules the app uses.
- **Web app:** React with TypeScript. It calls the core compiled to
  WebAssembly, and owns rendering, gestures, animation, storage and network.
- **A future native app** links the same crate (directly, or through Crux if
  that is worth it by then). Crux is not adopted now: it is pre-1.0 and only
  pays off once there is a native shell.

## Why

- The owner reads and writes Rust most comfortably, TypeScript second, and
  does not want JavaScript on the server.
- One set of rules on every side is what makes sync correct: the server runs
  the same reducer the phone ran.
- The core's tests are data (event lists in, state out), so they are readable
  in either language and carry over to any shell.

## Costs we accept

- Two languages in most features: rule changes in Rust, screen changes in
  TypeScript, with types generated across the boundary.
- A WebAssembly build step and a serialization boundary for every call from
  the app into the core.

## Rules out

- Domain logic in the UI layer. If a rule is needed on screen, the core
  exposes it.
- A TypeScript backend.

Research: `docs/research/2026-11-stack-sync-auth.md`, section 7.
