Load when you shape errors, ownership, conversions, control flow, or a public API, or when code fights the borrow checker, spams `.clone()`, or mishandles async. Why: idiomatic ownership and errors remove most clones, panics, and borrow fights before they spread. Type shapes, generics vs `dyn`: `references/types-and-patterns.md`.

# Idioms and gotchas

## Errors
- A fallible function returns `Result<T, E>` and propagates with `?`. No `.unwrap()`/`.expect()` in production paths; `expect` only for a true invariant, with a message that says why it cannot fail.
- Library: typed failure variants callers can inspect. Use an existing error type first; `thiserror` is useful when already available or a dependency is authorized. Preserve machine-readable reasons through host conversion.
- Binary: `anyhow::Result`, with `.context("what we were doing")` at each boundary.
- Convert at boundaries with `From`/`TryFrom` so `?` lifts the error; no `match` ladders that only remap errors.
- Combine `Option` with `?`, `ok_or`, `unwrap_or`, `map`, `and_then`.
- Never return a fake success from a catch-all (escalate via `octocode-clean-agentic-code`). `catch_unwind` is for FFI/thread boundaries, not error handling.

## Ownership and borrowing
- Return owned (`String`, `Vec<T>`) when you produce; `Cow<'_, str>` when the common path borrows.
- Moves and borrows before `Rc`/`Arc`; `Arc<Mutex<T>>` only when a channel does not express the design better.
- `String` is owned; `&str` is a view. Store `String`, convert with `.to_string()` / `&s[..]`. Frequent conversion is normal.

| Symptom | Fix |
|---------|-----|
| `.clone()` to silence the borrow checker | Borrow, move, split the scope, or use an index. A cold-path clone is fine. A hot path reuses capacity with `clone_from` (`references/performance-and-memory.md`). |
| `&String` / `&Vec<T>` params | Take `&str` / `&[T]`. |
| Borrow an element, then `push` to the same `Vec` | A realloc would dangle. Take the index, or end the borrow first. |
| Return a reference to a local | Return owned, or let the caller own the storage. |

## Control flow and iterators
- Prefer iterator traversal over manual index loops when it expresses the operation clearly; inspect generated code or benchmark before claiming a bounds-check or speed benefit.
- `for i in 0..v.len()` with `v[i]` → `for x in &v` or `.iter().enumerate()`.
- `.collect()` then one iteration → iterate the source.
- `if let` / `let ... else` / `while let` for one variant; `matches!` for a boolean test.
- Edition 2024 let chains can replace nested `if let`: `if let Some(x) = expr && let Some(y) = x.field() && cond`. Follow applicable `clippy::collapsible_if` diagnostics. A borrow used in the body remains live; a let chain does not itself make mutation of the borrowed collection safe.
- Return early with `?` and guard clauses.
- Shadowing can hide an unintended type change.

## Async
1. **Missing `.await`**: an `async fn` does nothing until awaited or spawned.
2. **Blocking in async**: `std::fs`, `std::net`, `std::process`, `std::thread::sleep`, `std::sync::mpsc`, or a CPU-heavy loop starves the executor thread. Use tokio equivalents, `tokio::task::spawn_blocking`, or rayon.
3. **`Mutex` guard across `.await`**: a `std::sync::Mutex` guard makes the future `!Send`, so it fails where a `Send` future is required. Local execution can still compile; blocking lock contention can then deadlock the executor. Keep synchronous lock sections short and await-free. Use `tokio::sync::Mutex` when a guard must cross `.await`.

One owner uses `join!` or `select!`. `select!` can cancel a future mid-operation, so that future must be cancellation-safe. A dynamic set uses `tokio::task::JoinSet`; dropping the set aborts its tasks. Work that must outlive the caller is `tokio::spawn` plus an explicit shutdown. Dropping a `JoinHandle` detaches the task and drops its result. An actor is that task, a handle, and a bounded channel (`references/performance-and-memory.md`). Skip an actor framework.

## Request lifetime: Octocode example
- Admit requests synchronously before scheduling asynchronous work. Reject closed runtimes, duplicate IDs, and a full pending queue at admission.
- An RAII guard owns request registration; a semaphore permit bounds running work. Dropping unstarted admission releases its registration.
- Pass cancellation and deadline through an execution context. Check them in long loops and before returning results.
- A started blocking task cannot be stopped by dropping its future. Signal cancellation and join it before reporting resource cleanup complete.
- Closing rejects new work, cancels existing requests, and waits for their guards to release. Keep mutex sections short and await-free.
- Test dropping admission, queue saturation, duplicate IDs, cancellation, timeout, and close while work still owns resources.

## API surface
- `Display` for user-facing text, `Debug` for developers.
- Follow the Rust API Guidelines (`references/sources-and-crates.md`): `as_`/`to_`/`into_` prefixes, `#[must_use]` on builders and pure results, sealed public traits (`references/types-and-patterns.md`).
- Document public items with `///` and a runnable example where it helps.
- Pedantic is per crate and warn-only, with each silenced lint justified. The workspace group policy is in `references/workspace.md`.
