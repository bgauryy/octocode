# Ownership, errors and APIs

Load for borrow errors, clones, conversions, recoverable failures, public API design, resource cleanup or lint suppression. For tasks and cleanup across suspension points, read [concurrency and async](concurrency-and-async.md).

## Errors and panics

- Return `Result<T, E>` for expected failure and propagate with `?`. Preserve the cause and the context needed to diagnose the failed operation; avoid logging the same failure at every layer.
- At a library or crate boundary, return a typed error (an enum, or a struct with a kind) that callers can match without parsing display text. `Result<_, String>` drops the source chain and turns every caller into a string parser. Reuse existing errors; `thiserror` is an optional derive helper. An application's top level may wrap errors with `anyhow`, a custom type or another established crate; none is mandatory.
- Use `From` for infallible conversions and `TryFrom` for fallible ones. Keep distinct failures distinct; a catch-all must not turn an error into empty data or success.
- Use `Option` for absence. Choose `ok_or_else`/`unwrap_or_else` when creating the fallback is expensive; eager forms are fine for cheap values.
- Follow the project's panic policy. Avoid `unwrap`/`expect` on user input, I/O and other expected failures. For a proven invariant, document why it holds and follow local lint policy; tests may use assertions and unwraps when allowed.
- `catch_unwind` catches unwinding panics, not aborts, and does not repair state. Reserve containment for an established boundary with an explicit failure policy; see [FFI](safety-and-ffi.md).

## Clean code

- Design APIs ownership-first: settle who owns each value and for how long, then choose types and signatures from the table below. Shared ownership, interior mutability or clones added to satisfy the compiler usually mean that decision is still open.
- Tie cleanup to `Drop`: guards for locks, temporary files and directories, child processes and restored state. Cleanup written after the work, in a function or a test, never runs when `?`, an early return, a panic or a failed assertion leaves first. Async cleanup still needs explicit completion; see [concurrency](concurrency-and-async.md).
- Pass configuration, clocks and hooks in. Process-global mutable state (`static` atomics or mutexes, environment variables read deep in the code) couples every caller in the process; in tests a leaked value fails unrelated tests ([testing](testing-and-tooling.md#write-trustworthy-tests)).
- Fix the code before suppressing a lint. When a suppression is right, use `#[expect(lint, reason = "...")]` on the smallest item instead of `#[allow]`: `expect` warns once the lint stops firing, so stale suppressions surface, and `reason` records why (Rust 1.81+; Clippy's `allow_attributes` and `allow_attributes_without_reason` enforce it).
- Module size and boundaries: see [crate structure](crate-structure.md#clean-modules).

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
- Doc comments state the contract, errors, panics and safety; delete comments that restate the code or narrate the change.
- `Debug` can expose secrets: derive it only when its output is safe, or redact sensitive fields. Use `Display` for the intended human representation.
- Add `#[must_use]` when silently ignoring a value is likely a mistake; it is a warning, not an execution or cleanup guarantee.
- Keep public compatibility deliberate. Sealing a trait forbids downstream implementations; use it only when that restriction is intended. A new error variant or public field can affect downstream exhaustive code.

Sources: [The Book: error handling](https://doc.rust-lang.org/book/ch09-00-error-handling.html), [API Guidelines](https://rust-lang.github.io/api-guidelines/), [`expect` attribute](https://doc.rust-lang.org/reference/attributes/diagnostics.html#the-expect-attribute), [Clippy `allow_attributes_without_reason`](https://rust-lang.github.io/rust-clippy/master/index.html#allow_attributes_without_reason), [let chains](https://doc.rust-lang.org/edition-guide/rust-2024/let-chains.html), [Rust 1.88 release](https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/).
