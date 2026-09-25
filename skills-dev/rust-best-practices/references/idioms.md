# Idioms — writing Rust that reads as Rust

Load when shaping errors, ownership, conversions, control flow, or a public API. Why: idiomatic surface is what makes Rust safe *and* pleasant; the compiler enforces safety, idioms enforce clarity.

## Error handling
- Functions that can fail return `Result<T, E>`; propagate with `?`, never `.unwrap()`/`.expect()` in production paths. `expect` is allowed only for a true invariant, and its message must state why it cannot fail.
- **Library:** define a typed error enum with `thiserror`; one variant per failure mode; `#[from]` for automatic conversion of source errors.
- **Binary:** use `anyhow::Result`; add `.context("what we were doing")` at each boundary so the chain reads top-down.
- Convert at boundaries with `From`/`TryFrom` so `?` does the lifting; avoid manual `match` ladders that only remap errors.
- Model absence with `Option`, not sentinel values; combine with `?`, `ok_or`, `unwrap_or`, `map`, `and_then`.

## Ownership & borrowing
- Parameters: take `&str` (not `&String`), `&[T]` (not `&Vec<T>`) — accepts more callers, decouples storage.
- Return owned (`String`, `Vec<T>`) when you produce; return `Cow<'_, str>` for "usually borrow, sometimes own" (see `references/performance.md`).
- Don't `.clone()` to silence the borrow checker — restructure ownership, borrow, or use an index. Clone-to-satisfy is a documented anti-pattern (`references/gotchas.md`).
- Prefer moves and borrows over `Rc`/`Arc` until sharing is genuinely required; reach for `Arc<Mutex<T>>` only when a channel doesn't express the design better.

## Types that make bad states impossible
- **Newtypes** for units/IDs: `struct UserId(u64)` beats a bare `u64` — no accidental mixups.
- Model exclusive states as an `enum`, not a bag of `Option`/`bool` fields; then `match` is exhaustive and the compiler catches new variants.
- Prefer `impl Trait` in argument and simple return position over boxing; box (`Box<dyn Trait>`) only for heterogeneous collections or to break type recursion.

## Iterators & control flow
- Prefer iterator chains (`.iter().filter().map().collect()`) over manual index loops — zero-cost, bounds-checks elided, intent-revealing.
- Use `if let` / `let ... else` / `while let` for single-variant matches; `matches!` for boolean tests.
- Return early with `?` and guard clauses; avoid deep nesting.

## API surface
- Derive `Debug` on ~everything; add `Clone, PartialEq, Eq, Hash` when semantically valid. Derive `Default` over a hand-written `new()` when fields have obvious zeros.
- Implement `Display` for user-facing text, keep `Debug` for developers.
- Follow the Rust API Guidelines (the official public-API checklist — `references/canonical-sources.md`): predictable naming (`as_`/`to_`/`into_` conversion prefixes), `#[must_use]` on builders and pure results, sealed traits for future-proof public traits (`references/design-patterns.md`).
- Gate the public surface: `pub(crate)` by default, `pub` only what you commit to. Document public items with `///` and a runnable example where it helps.

## Baseline enforcement
- `cargo fmt` (checked in CI with `--check`) and `cargo clippy -- -D warnings` decide the mechanical idioms — run them before hand-review. Consider `#![warn(clippy::pedantic)]` per-crate and silence individual lints with justification.

Next: for the pattern behind an invariant-enforcing type, load `references/design-patterns.md`; for the anti-pattern version of any rule here, `references/gotchas.md`.
