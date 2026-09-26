# Testing — layout, kinds, and the test loop

Load when adding tests, choosing a test kind, organizing test files, or making a suite faster or more trustworthy. Why: Rust gives unit, integration, and doc tests for free, but layout mistakes (one binary per `tests/*.rs`, `unwrap` noise, hidden global state) make suites slow and flaky.

## Layout
- **Unit tests** beside the code: `#[cfg(test)] mod tests { use super::*; … }` — reach private items. When they outgrow the file, `#[cfg(test)] mod tests;` → `foo/tests.rs`.
- **Integration tests** in `tests/` see only the public API. Every `tests/*.rs` is a separate crate that links the whole library — prefer **one binary**: `tests/it/main.rs` + `mod`s (matklad, "Delete Cargo Integration Tests"). Shared helpers: `tests/it/support/mod.rs`, not `tests/common.rs`.
- **Doc tests** on every public item's example; they are the docs' guarantee. Use `no_run`/`ignore` sparingly and never to hide rot.
- `examples/` compile under `cargo test` — keep them building. Benches in `benches/` with `harness = false` (criterion/divan).
- Test data: `concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/…")` or `include_str!`; scratch files in `tempfile::TempDir`, never the repo or `/tmp` by name.
- Fakes other crates need: expose behind `#[cfg(any(test, feature = "test-util"))]` (tokio's pattern) or a `publish = false` `*-test-support` dev-dependency.

## Write them well
- Test behavior through the API, one reason to fail per test, name states the rule (`rejects_path_outside_root`).
- Return `Result<(), Box<dyn Error>>` and use `?`; set `allow-unwrap-in-tests = true` / `allow-expect-in-tests = true` in `clippy.toml` when the workspace denies `unwrap_used`.
- Deterministic: inject clock/RNG/env/filesystem roots; no network; `#[tokio::test(start_paused = true)]` (tokio `test-util`) for timeouts. `serial_test` only for unavoidable process-global state (env vars, cwd) — and prefer removing that state.
- Mock with a hand-written fake behind a trait; `mockall` only when a fake is impractical.

## Pick the kind
| Need | Tool |
|---|---|
| Output of parser/formatter/CLI | `insta` snapshots (+ redactions for paths/times) |
| Invariants over many inputs | `proptest` (commit `proptest-regressions/`) |
| Untrusted bytes | `cargo fuzz` targets; turn each crash into a regression unit test |
| CLI end-to-end | `assert_cmd` + `predicates`, or `snapbox`/`trycmd` for file-driven cases |
| "This must not compile" (macros, typestate) | `trybuild` |
| `unsafe` / lock-free code | Miri, `loom` |
| Async | `#[tokio::test]`; `flavor = "multi_thread"` only when the code needs it |

## Run and gate
- `cargo nextest run --workspace` (fast, isolated) + `cargo test --doc` (nextest skips doctests).
- Features: `cargo hack test --each-feature` for libs with flags; always also `--all-features` and `--no-default-features`.
- Trust: `cargo llvm-cov nextest` for coverage gaps, `cargo mutants` to prove assertions bite. Coverage % is a map, not a goal.
- Node addons: test the core crate in Rust; test the binding from JS (vitest/ava) against the built `.node`; the thin binding crate sets `[lib] test = false, doctest = false` (oxc does).

Next: for the tools themselves, load `references/dev-tooling.md`; for Miri/fuzz in the release gate, `references/safety-and-security.md`.
