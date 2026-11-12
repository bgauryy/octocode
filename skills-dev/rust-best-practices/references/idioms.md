# Ownership, errors and APIs

Load for borrow errors, clones, conversions, recoverable failures or public API design. For tasks and cleanup across suspension points, read [concurrency and async](concurrency-and-async.md).

## Errors and panics

- Return `Result<T, E>` for expected failure and propagate with `?`. Preserve the cause and the context needed to diagnose the failed operation; avoid logging the same failure at every layer.
- In a library, expose errors callers can act on without parsing display text. Reuse existing errors; `thiserror` is an optional derive helper. In an application, the established error wrapper may be `anyhow`, a custom type or another crate; none is mandatory.
- Use `From` for infallible conversions and `TryFrom` for fallible ones. Keep distinct failures distinct; a catch-all must not turn an error into empty data or success.
- Use `Option` for absence. Choose `ok_or_else`/`unwrap_or_else` when creating the fallback is expensive; eager forms are fine for cheap values.
- Follow the project's panic policy. Avoid `unwrap`/`expect` on user input, I/O and other expected failures. For a proven invariant, document why it holds and follow local lint policy; tests may use assertions and unwraps when allowed.
- `catch_unwind` catches unwinding panics, not aborts, and does not repair state. Reserve containment for an established boundary with an explicit failure policy; see [FFI](safety-and-ffi.md).

## Ownership decisions

| Need or symptom | Action |
|---|---|
| Read text, bytes or a sequence without retaining it | Accept `&str`, `&[u8]` or `&[T]`; use a more specific type when its extra capability matters. |
| Retain or produce a value | Move/return owned data. Borrowed struct fields are appropriate for views whose owner outlives them. |
| Usually borrow, sometimes transform | Consider `Cow`; do not add lifetime complexity when an owned result is clearer. |
| Clone introduced to satisfy borrowing | Try shorter scopes, separate field borrows, moving, `Option::take`, or indices first. Keep intentional clones when semantics and cost justify them. |
| Mutate a collection while holding an element reference | End the borrow or store a stable identifier/index with the required invalidation checks. Never bypass the borrow rules with a pointer cast. |
| Shared ownership | Choose `Rc`/`Arc` only when multiple owners need the value; choose synchronization separately. |

## Control flow and public contracts

- Use iterators, loops and pattern matching according to clarity. Avoid collecting an intermediate sequence only to iterate it once unless ownership or reuse requires it.
- Prefer guard clauses, `?`, `if let` and `let ... else` when they simplify the paths. Check syntax and std API availability against MSRV. Let chains require edition 2024 and Rust 1.88 or later; nested matching remains valid for older projects.
- Follow the API Guidelines' `as_`/`to_`/`into_` conversion conventions. Document public errors, panic conditions and safety preconditions; include runnable examples where they clarify usage.
- `Debug` can expose secrets: derive it only when its output is safe, or redact sensitive fields. Use `Display` for the intended human representation.
- Add `#[must_use]` when silently ignoring a value is likely a mistake; it is a warning, not an execution or cleanup guarantee.
- Keep public compatibility deliberate. Sealing a trait forbids downstream implementations; use it only when that restriction is intended. A new error variant or public field can affect downstream exhaustive code.

Sources: [The Book: error handling](https://doc.rust-lang.org/book/ch09-00-error-handling.html), [API Guidelines](https://rust-lang.github.io/api-guidelines/), [let chains](https://doc.rust-lang.org/edition-guide/rust-2024/let-chains.html), [Rust 1.88 release](https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/).
