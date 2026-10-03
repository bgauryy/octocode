# Testing and tooling

Load when you add tests, choose a test kind, organize test files, make a suite faster or more trustworthy, set up a dev loop, CI, or a new contributor, or ask "is there a tool for X?". Install prebuilt binaries with `cargo binstall <tool>`; check a tool is maintained before adopting it.

## Test layout
- **Unit tests** beside the code: `#[cfg(test)] mod tests { use super::*; … }` — reach private items. When they outgrow the file, `#[cfg(test)] mod tests;` → `foo/tests.rs`.
- **Integration tests** in `tests/` see only the public API. Every `tests/*.rs` is a separate crate that links the whole library — prefer **one binary**: `tests/it/main.rs` + `mod`s (matklad, "Delete Cargo Integration Tests"). Shared helpers: `tests/it/support/mod.rs`, not `tests/common.rs`.
- **Doc tests** on every public item's example; they are the docs' guarantee. Use `no_run`/`ignore` sparingly and never to hide rot.
- `examples/` compile under `cargo test` — keep them building. Benches in `benches/` with `harness = false` (criterion/divan).
- Test data: `concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/…")` or `include_str!`; scratch files in `tempfile::TempDir`, never the repo or `/tmp` by name.
- Fakes other crates need: expose behind `#[cfg(any(test, feature = "test-util"))]` (tokio's pattern) or a `publish = false` `*-test-support` dev-dependency.

## Write tests well
- Test behavior through the API, one reason to fail per test, name states the rule (`rejects_path_outside_root`).
- Return `Result<(), Box<dyn Error>>` and use `?`; set `allow-unwrap-in-tests = true` / `allow-expect-in-tests = true` in `clippy.toml` when the workspace denies `unwrap_used`.
- Deterministic: inject clock/RNG/env/filesystem roots; no network; `#[tokio::test(start_paused = true)]` (tokio `test-util`) for timeouts. `serial_test` only for unavoidable process-global state (env vars, cwd) — and prefer removing that state.
- Mock with a hand-written fake behind a trait; `mockall` only when a fake is impractical.

| Need | Test tool |
|---|---|
| Output of parser/formatter/CLI | `insta` snapshots (+ redactions for paths/times) |
| Invariants over many inputs | `proptest` (commit `proptest-regressions/`) |
| Untrusted bytes | `cargo fuzz` targets; turn each crash into a regression unit test |
| CLI end-to-end | `assert_cmd` + `predicates`, or `snapbox`/`trycmd` for file-driven cases |
| "This must not compile" (macros, typestate) | `trybuild` |
| `unsafe` / lock-free code | Miri, `loom` |
| Async | `#[tokio::test]`; `flavor = "multi_thread"` only when the code needs it |

- Run: `cargo nextest run --workspace` (fast, isolated) + `cargo test --doc` (nextest skips doctests).
- Features: `cargo hack test --each-feature` for libs with flags; always also `--all-features` and `--no-default-features`.
- Trust: `cargo llvm-cov nextest` for coverage gaps, `cargo mutants` to prove assertions bite. Coverage % is a map, not a goal.
- Node addons: test the core crate in Rust; test the binding from JS (vitest/ava) against the built `.node`; the thin binding crate sets `[lib] test = false, doctest = false` (oxc does).

## Toolchain pin (commit it)
```toml
# rust-toolchain.toml
[toolchain]
channel = "1.90"        # exact stable CI builds with; MSRV floor lives in rust-version
components = ["rustfmt", "clippy", "rust-analyzer", "llvm-tools"]
```
Built-ins: `cargo add/remove/info`, `cargo tree -d -i <crate>`, `cargo fix --edition`, `cargo doc --open`, `cargo build --timings`, `cargo metadata` (for scripts).

## The belt
| Job | Tool | Note |
|---|---|---|
| IDE | `rust-analyzer` | set `check.command = "clippy"` so the editor shows lints |
| Watch loop | `bacon` | background check/clippy/test; `cargo-watch` is in maintenance mode |
| Format | `rustfmt` (+ `rustfmt.toml`), `taplo fmt` for TOML, `cargo sort` for deps | zero-debate diffs |
| Lint | `clippy` with `[workspace.lints]`; `typos` for spelling | first review pass |
| Tests | `cargo nextest run` | per-test processes, parallel, retries, JUnit; doctests via `cargo test --doc` |
| Coverage / test quality | `cargo llvm-cov nextest` · `cargo mutants` | source-based lcov/html; proves tests assert |
| UB / concurrency | `cargo +nightly miri test` · `loom` · `kani` | UB, interleavings, bounded proofs |
| Inspect codegen | `cargo expand` · `cargo asm` (cargo-show-asm) · `cargo llvm-lines` | macro output, inlining, generic bloat |
| Profile CPU / size | `samply` · `cargo flamegraph` · `perf` · `cargo bloat --release --crates` | build with a `profiling` profile |
| Profile memory | `dhat-rs` · `heaptrack` | `references/performance-and-memory.md` |
| Bench / async | `criterion` / `divan` · `hyperfine` (CLI) · `tokio-console` | `--release` numbers; stuck tasks, busy polls |
| Unused deps / disk | `cargo machete` · `cargo shear` · `cargo udeps` (nightly) · `cargo sweep` | trim graph and stale `target/` |
| Features | `cargo hack --each-feature` / `--feature-powerset` | every combo compiles |
| API / MSRV | `cargo semver-checks` · `cargo msrv verify` | no accidental breaking release |
| Supply chain | `cargo deny` · `cargo audit` · `cargo vet` · `cargo auditable` | `references/safety-and-ffi.md` |
| Release | `release-plz` / `cargo-release` · `cargo-dist` | version bump, changelog, binaries |

## Aliases = npm scripts
```toml
# .cargo/config.toml
[alias]
lint = "clippy --workspace --all-targets --all-features -- -D warnings"
t    = "nextest run --workspace"
xtask = "run -p xtask --"
```
Anything beyond one command goes in an `xtask` crate, not bash.

## CI order (fail fast, cheap first)
`fmt --check` → `clippy -D warnings` → `nextest` + `test --doc` → `deny check` → `hack --each-feature check` → `semver-checks` (libs) → MSRV build → Miri/fuzz on unsafe/parsers. Use `--locked` everywhere; cache with `Swatinem/rust-cache` (+ `sccache` per `references/build-profiles.md`). The merge-blocking security subset is in `references/safety-and-ffi.md`.
