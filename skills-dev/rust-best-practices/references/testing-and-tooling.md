# Testing and tooling

Load when you add tests, choose a test kind, organize test files, make a suite faster or more trustworthy, set up a dev loop, CI, or a new contributor, or ask "is there a tool for X?". Why: test layout and tooling set both suite speed and how much a green run proves. Install prebuilt binaries with `cargo binstall <tool>`; check a tool is maintained before adopting it.

## Test layout
- **Unit tests** beside the code: `#[cfg(test)] mod tests { use super::*; … }` — reach private items. When they outgrow the file, `#[cfg(test)] mod tests;` → `foo/tests.rs`.
- **Integration tests** in `tests/` see only the public API. Every `tests/*.rs` is a separate crate that links the whole library — prefer **one binary**: `tests/it/main.rs` + `mod`s (matklad, "Delete Cargo Integration Tests"), or keep files in place with `tests/main.rs`, `autotests = false`, `[[test]] path = "tests/main.rs"`, plus a test asserting every `tests/*.rs` is declared. Measured: 31 → 6 executables, −80% bytes per build. Shared helpers: one `support/mod.rs` module, not `tests/common.rs`.
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
| Async | `#[tokio::test]`; `flavor = "multi_thread"` only when the code needs it |

- Coverage % helps locate untested behavior. Preserve repository coverage floors; never lower one to pass a change.
- Node addons: test the binding from JS (vitest/ava) against the built `.node`.

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
| Lint | `clippy` with `[workspace.lints]`; `typos` for spelling | |
| Tests | `cargo nextest run --workspace` | per-test processes, parallel, retries, JUnit; nextest skips doctests, so also run `cargo test --doc` |
| Coverage / test quality | `cargo llvm-cov nextest` · `cargo mutants` | source-based lcov/html; proves tests assert |
| UB / concurrency | `cargo +nightly miri test` · `loom` · `kani` | UB, interleavings, bounded proofs |
| Inspect codegen | `cargo expand` · `cargo asm` (cargo-show-asm) · `cargo llvm-lines` | macro output, inlining, generic bloat |
| Profile CPU / memory / size | `references/performance-and-memory.md` | build with a `profiling` profile |
| Bench / async | `criterion` / `divan` · `hyperfine` (CLI) · `tokio-console` | stuck tasks, busy polls |
| Unused deps / disk | `cargo machete` · `cargo shear` · `cargo udeps` (nightly) · `cargo clean --workspace --profile dev` | trim graph; reset stale `target/` copies (`references/build-profiles.md`; `cargo sweep` is unmaintained) |
| Features | `cargo hack test --each-feature` / `--feature-powerset`, plus `--all-features` and `--no-default-features` | every combo compiles (libs with flags) |
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

## Octocode verification
- Use `octocode-dev` tasks and existing workspace scripts; do not introduce a second task runner.
- Run `yarn workspace @octocodeai/octocode-native test:rust` for the repo's package/feature selections. Test-only N-API stubs stay out of production builds.
- After native changes: rebuild native and affected interfaces, run CLI `config --json` and `schema`, then call the changed tool. Restart MCP before checking it.
- For pagination changes, walk every executable `next.*` page. Assert complete coverage exactly once, stable query identity, and rejection or safe replay after source edits.
- Cover errors, warnings, and partial rows through CLI/MCP as well as Rust. A green exit or Rust-only test does not prove every tool row succeeded.

## CI order (fail fast, cheap first)
`fmt --check` → `clippy --all-targets -D warnings` → `nextest` + `test --doc` → `audit` (RUSTSEC) + `deny check` (licenses, bans, advisories, duplicates) → `hack --each-feature check` → `semver-checks` (libs) → MSRV build → Miri on unsafe-bearing crates, `cargo fuzz` on parsers/decoders of untrusted bytes. Block merge on fmt through deny plus Miri/fuzz. Cache with `Swatinem/rust-cache` (+ `sccache` per `references/build-profiles.md`).
