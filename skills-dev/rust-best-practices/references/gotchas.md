# Gotchas — anti-patterns and their idiomatic fix

Load when code fights the borrow checker, spams `.clone()`, or mishandles async. Why: these are the recurring footguns; each has a known idiomatic fix, not a workaround.

## Ownership & borrowing
| Symptom | Fix |
|---------|-----|
| `.clone()` sprinkled to silence the borrow checker | Restructure ownership: borrow, move, split the scope, or use an index. Clone-to-satisfy is a documented anti-pattern and a top perf killer. |
| `&String` / `&Vec<T>` params | Take `&str` / `&[T]` — accepts more callers, no coupling. |
| Borrow an element then `push` to the same `Vec` | Not allowed — a realloc would dangle. Take the index, or finish the borrow before mutating. This is the checker protecting you, not obstructing you. |
| Returning a reference to a local | Return owned, or restructure so the caller owns the backing storage. |
| Reaching for `Rc<RefCell<T>>` graphs | Usually a design smell — prefer arena + indices, or `Arc` + message passing. |

## String vs &str
- `String` is owned/growable; `&str` is a borrowed view. Take `&str`, store `String`, convert with `.to_string()` / `&s[..]`. You'll do it constantly — it's normal, not a code smell.

## Panics & error masking
- `.unwrap()` / `.expect()` in production paths panic on the unhappy path. Propagate with `?`; use `unwrap_or`, `ok_or`, `if let`, `let ... else`. `expect` only for a true invariant, with a message explaining why it can't fail.
- Don't swallow errors in a catch-all that returns a fake success — that disguises failure (escalate via `octocode-clean-agentic-code`, don't delete).
- `panic = "unwind"` + `catch_unwind` is not error handling — it's for FFI/thread boundaries.

## Async pitfalls (the big four)
1. **Forgetting `.await`** — an `async fn` builds a state machine and does nothing until awaited. A future you never `.await` (or `spawn`) silently never runs.
2. **Blocking in async** — calling `std::fs`, `std::net`, `std::thread::sleep`, or a CPU-heavy loop inside a task starves the executor thread. Use tokio's async equivalents, or `tokio::task::spawn_blocking` / rayon for CPU work.
3. **Holding a `Mutex` guard across `.await`** — the task can suspend while holding the lock; anything else needing it stalls, worst case forever. Keep sync `std::sync::Mutex` critical sections short and await-free; only use `tokio::sync::Mutex` when you *must* hold across an await (it's heavier — don't reach for it by habit).
4. **`Arc<Mutex<T>>` reflex** — often a channel (`mpsc`) models the flow better: transfer ownership of a message instead of sharing mutable state.

Further async traps: unbounded channels leaking memory under backpressure; `select!` branches that cancel a future mid-operation (cancellation-safety); spawning without holding the `JoinHandle` and losing the panic/result.

## Iterator/collection traps
- `.collect()` then iterate once — often removable; iterate the source directly.
- `for i in 0..v.len()` with `v[i]` — prefer `for x in &v` or `.iter().enumerate()`; avoids bounds checks and off-by-one.
- Shadowing that hides a type change you didn't intend — read shadowed bindings carefully.

Next: for the ownership design that avoids these, load `references/idioms.md`; for the allocation cost of clone-heavy code, `references/performance.md`.
