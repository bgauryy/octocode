Load when you add tests, choose a test kind, organize test files, make a suite faster or more trustworthy, set up a dev loop, CI, or a new contributor, or ask "is there a tool for X?". Why: test layout and tooling set both suite speed and how much a green run proves. Install prebuilt binaries with `cargo binstall <tool>`. A from-source `cargo install` uses `--locked` when the crate requires it (nextest does). Check a tool is maintained before adopting it.

# Testing and tooling

## Test layout
- **Unit tests** beside the code: `#[cfg(test)] mod tests { use super::*; … }` — reach private items. When they outgrow the file, `#[cfg(test)] mod tests;` → `foo/tests.rs`.
- **Integration tests** in `tests/` see only the public API. Cargo normally builds each `tests/*.rs` as a separate target. When target startup or link cost matters, group related cases under `tests/it/main.rs` with modules, or keep files in place with `tests/main.rs`, `autotests = false`, and `[[test]] path = "tests/main.rs"`. If using manual registration, check that every intended test module is included. Shared helpers belong in `support/mod.rs`, not a top-level `tests/common.rs` target.
- **Doc tests** for public examples that readers should be able to run; they are the examples' guarantee. Use `no_run`/`ignore` only when execution genuinely needs an unavailable environment.
- `examples/` compile under `cargo test` — keep them building. Benches in `benches/` with `harness = false` (criterion/divan).
- Test data: `concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/…")` or `include_str!`; scratch files in `tempfile::TempDir`, never the repo or `/tmp` by name.
- Fakes other crates need: expose behind `#[cfg(any(test, feature = "test-util"))]` (tokio's pattern) or a `publish = false` `*-test-support` dev-dependency.

## Write tests well
- Test behavior through the API, one reason to fail per test, name states the rule (`rejects_path_outside_root`).
- Return `Result<(), Box<dyn Error>>` and use `?`. When the workspace denies `unwrap_used` and `expect_used`, set both `allow-unwrap-in-tests` and `allow-expect-in-tests` in `clippy.toml`. They are separate keys, and each covers `#[cfg(test)]`, `tests/`, and `benches/`. A library `#![cfg_attr(test, allow(...))]` covers only that crate's unit tests. `clippy::panic` has no matching key, so an integration test or a bench that calls `panic!` needs its own allow.
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
A file that sets only `channel` does not pin components. rustup then installs that machine's profile: `default` includes rustfmt and clippy and omits `rust-analyzer` and `llvm-tools`; `minimal` (typical CI) omits rustfmt and clippy as well ([rustup profiles](https://rust-lang.github.io/rustup/concepts/profiles.html)). List `components` in the toolchain file. Leave the channel and the `rust-version` floor alone unless that bump is the task (`references/workspace.md`).

Built-ins: `cargo add/remove/info`, `cargo tree -d -i <crate>`, `cargo fix --edition`, `cargo doc --open`, `cargo build --timings`, `cargo metadata` (for scripts).

## The belt
| Job | Tool | Note |
|---|---|---|
| IDE | `rust-analyzer` | `check.command` stays `"check"` (the default). `"clippy"` only when save stays fast |
| Watch loop | `bacon` or `watchexec` | `bacon` is AGPL-3.0. `cargo-watch` is archived; its README says use Bacon. `watchexec` when that license does not fit |
| Format | `rustfmt` (+ `rustfmt.toml`), `taplo fmt` for TOML, `cargo sort` for deps | zero-debate diffs |
| Lint | `clippy` with `[workspace.lints]`; `typos` for spelling | |
| Tests | `cargo nextest run --workspace` | per-test processes, parallel, retries, JUnit; nextest skips doctests, so also run `cargo test --doc` |
| Coverage / test quality | `cargo llvm-cov nextest` · `cargo mutants` | `llvm-cov` needs the `llvm-tools` component, which the default rustup profile omits. `cargo mutants` is spare-time: a schedule or the files you changed, not every save |
| UB / concurrency | `cargo +nightly miri test` · `cargo careful` · `loom` · `kani` | behavior, not an `unsafe` count. Kani does not support `await`. Off the edit loop |
| Inspect codegen | `cargo expand` · `cargo asm` (cargo-show-asm) · `cargo llvm-lines` | macro output, inlining, generic bloat |
| Profile CPU / memory / size | `references/performance-and-memory.md` | build with a `profiling` profile |
| Bench / async | `criterion` / `divan` · `hyperfine` (CLI) · `tokio-console` | stuck tasks, busy polls |
| Unused deps / disk | `cargo shear` · `cargo machete` · `cargo udeps` (nightly) · `cargo clean --workspace --profile dev` | rules below; reset stale `target/` copies (`references/build-profiles.md`) |
| Features | `cargo hack test --each-feature` / `--feature-powerset`, plus `--all-features` and `--no-default-features` | every combo compiles (libs with flags) |
| API / MSRV | `cargo semver-checks` · `cargo msrv verify` | no accidental breaking release |
| Supply chain | `cargo deny` · `cargo audit` · `cargo vet` · `cargo auditable` | `references/safety-and-ffi.md` |
| Release | `release-plz` / `cargo-release` · `cargo-dist` | version bump, changelog, binaries |

- Editor check-on-save uses `cargo check` ([rust-analyzer configuration](https://rust-analyzer.github.io/book/configuration.html)). The manifest floor makes that fail rustc warnings (`references/workspace.md`). Clippy lints still need `cargo clippy`. Set `check.command` to `"clippy"` only when that save stays fast (`references/build-profiles.md`). A language server the product ships to its users is a different program; leave its check at the product setting.
- `rustfmt.toml` stays on stable keys. Unstable keys (`imports_granularity`, `group_imports`, `wrap_comments`, and the other nightly-only options) are ignored on the stable channel and warn on every format.
- Unused dependencies: `cargo shear` is the accurate stable scan, including dev and build dependencies. `cargo machete` is the fast text scan of normal dependencies in lib, bin, test, bench, and example sources. It does not open `build.rs`, and the default scan skips `[build-dependencies]` and `[dev-dependencies]`. It misses uses that exist only in generated code, and it false-positives a crate whose Rust name is not its package name. `cargo udeps` is the nightly compiler-grounded pass. Read `build.rs` and generated output before you delete a hit. Record a real machete false positive in `[package.metadata.cargo-machete] ignored`; use `renamed` when the Rust name differs. `--with-metadata` resolves renames and can rewrite `Cargo.lock`. `--fix` deletes every hit, including false positives.

## Two speeds
The edit loop and the handoff gate use different feature selections, so they compile different unit graphs (`references/build-profiles.md`).

Typecheck the crate you changed, with the same features as the dev build and the test command:

```bash
cargo check -p <crate> --all-targets --features <dev-unify>
```

Format the files you touched with `cargo fmt`. When you want lints before handoff, run Clippy on that same selection instead of `cargo check`, not after it. Clippy already typechecks.

```bash
cargo clippy -p <crate> --all-targets --features <dev-unify> -- -D warnings
```

In Octocode, match the selections in `packages/octocode-native/scripts/rust-tests.cjs`: the runtime, GitHub, I/O, and CLI packages with `octocode-native/dev-unify`, then `octocode-engine --all-features`.

For a workspace that uses this gate, run it once at handoff; otherwise follow its own CI commands and feature selection:

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

## Aliases = npm scripts
The `lint` alias is the gate, not the edit loop.

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
- Run `yarn workspace @octocodeai/octocode-native test:rust` for the repo's package/feature selections; inspect `scripts/rust-tests.cjs` before copying them into another command.
- After native changes: rebuild native and affected interfaces, run CLI `config --json` and `schema`, then call the changed tool. Restart MCP before checking it.
- For pagination changes, walk every executable `next.*` page. Assert complete coverage exactly once, stable query identity, and rejection or safe replay after source edits.
- Cover errors, warnings, and partial rows through CLI/MCP as well as Rust. A green exit or Rust-only test does not prove every tool row succeeded.
- `lint:ci` and `test:ci` exclude `@octocodeai/octocode-native`, so a green run of those tasks does not prove the native gate. Run the native package gate locally.

## CI order (fail fast, cheap first)
`fmt --check` → `clippy --all-targets -D warnings` → `nextest` + `test --doc` → `audit` (RUSTSEC) + `deny check` (licenses, bans, advisories, duplicates) → `hack --each-feature check` → `semver-checks` (libs) → MSRV build → Miri on unsafe-bearing crates, `cargo fuzz` on parsers/decoders of untrusted bytes. Block merge on fmt through deny, plus Miri and fuzz where those crates exist. Miri, `cargo careful`, and Kani stay off the edit loop. Cache with `Swatinem/rust-cache` (+ `sccache` per `references/build-profiles.md`).
