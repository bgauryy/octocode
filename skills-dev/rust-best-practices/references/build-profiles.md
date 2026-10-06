# Build profiles, compile time, and target/

Load when tuning `[profile.*]`, cutting dev/CI compile time, or when `target/` balloons or a step rebuilds what another step just built. Why: every profile, feature, and package choice decides how many copies of each crate Cargo compiles and keeps forever. Profiles take effect **only in the workspace-root** `Cargo.toml`.

## Release
```toml
[profile.release]
lto = "thin"          # cross-crate optimization with lower link cost; benchmark fat as an alternative
codegen-units = 1     # max optimization on hot crates (see per-package override)
opt-level = 3         # "s"/"z" when size dominates (CLI, wasm, addons) — benchmark both
strip = "symbols"
# Keep the existing panic strategy; an addon abort terminates its Node host.
```
- `catch_unwind` cannot catch an aborting panic. Cargo test harnesses require unwinding and ignore the profile's `panic` setting; ordinary abort builds still terminate the process. [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#panic).
- `codegen-units = 1` serializes a crate's codegen. Keep it on hot-CPU crates; give IO/glue crates `[profile.release.package.<crate>] codegen-units = 16`. Find "hot" with `--timings` + a benchmark.
- Treat another project's LTO choice as a candidate, not a rule. Octocode ships thin LTO; preserve it without a measured reason to change.
- Octocode keeps engine codegen units at 1 and runtime units at 16. The root manifest records a minify regression from engine units 8/16; remeasure before changing that override.
- Profiling: `[profile.profiling] inherits = "release"`, `debug = true`, `strip = false`, `lto = false`; build with `--profile profiling`.

## Dev and test profiles
```toml
[profile.dev]
debug = "line-tables-only"   # file:line in backtraces, far less DWARF (perf book: 20-40% faster)
split-debuginfo = "unpacked" # macOS: skip dsymutil on every link; keeps per-CGU .o files in deps/
[profile.dev.package."*"]
debug = false                # dependencies: no debuginfo
[profile.dev.package."regex-automata"]
opt-level = 3                # a dep that dominates runtime; own crates stay opt-level 0
```
- `[profile.test]` inherits dev **including package overrides**: never duplicate `[profile.dev.package.X]` under `[profile.test.package.X]` (verified: 0 differences over 418 units after deleting 30 duplicate lines).
- The profile *name* is not in the unit hash: dev and test units are shared when their settings match.

## Dev-loop compile time
- Diagnose with `cargo build --timings`; read the **critical path**, not the slowest crate. A heavy dep off the serial tail saves ~0 (`aws-lc-sys` → `ring` saves nothing). Attack your own members on the tail.
- Linker: lld is the Linux default since 1.90, and mold or wild is optional there. On macOS keep Apple ld: measured relink 1.2–1.6 s Apple vs 1.4–1.8 s lld, no win.
- `sccache` skips incremental (own-crate) compiles: "Non-cacheable: incremental". Its value is deps and C/C++ (tree-sitter, aws-lc) across clean builds, new `CARGO_TARGET_DIR`s, and CI. Check `sccache --show-stats` before you claim a win.
- Cranelift and `-Zthreads` are nightly only.

## One build, no copies: share units across steps
A unit's hash covers package, target, profile settings, features, and its deps' hashes. Every distinct combination is a separate compile and a separate copy on disk. Copies come from:
- **Different `-p` selections or `--all-features`.** Feature unification is per command (resolver 2).
- **Dev-dependency features.** These leak only into test builds. Example: `wiremock` turns on hyper `server`/`http2`, which rehashes hyper → reqwest → your crates → the bin, so `cargo test` recompiled everything `build:dev` had just built.
- **Test-only features** (napi `noop`) and `RUSTFLAGS`/env differences.

Measure; don't guess:
- `cargo +nightly build --unit-graph -Z unstable-options`, the same for `test --no-run`, then diff unit keys with the profile name excluded. This is deterministic and needs no compile.
- `cargo tree -e features -i <crate>` per selection shows who turns a feature on.
- `cargo build -v` prints `Dirty <crate>: <reason>` for unexpected rebuilds.

Fix, in order:
1. Give build, test and lint the same package and feature selection. Merge test commands that differ only in packages; never run `cargo check` after `clippy` with the same selection (clippy already checks).
2. Unify dev-dep features through an opt-in feature that dev builds and tests enable and release never does:
   ```toml
   [features]
   dev-unify = ["dep:hyper", "tokio-util/codec"]   # mirror what dev-deps enable
   [dependencies]
   hyper = { version = "1", features = ["full"], optional = true }
   ```
   Measured: test-only units 47 → 15 (only real test deps remain). The test build after `build:dev` went from 66 s to 19 s. Release binaries stay byte-identical; the always-on version cost +1.2% binary size.
- Rejected after prototypes:
  - `cargo-hakari`: it unified napi `noop` (a test stub) into the shipped addon and left hyper split (47 → 39).
  - `-Zfeature-unification` / `resolver.feature-unification = "workspace"`: nightly-only and ignores dev-deps (52).

## target/ hygiene
- Stable Cargo has no `target/` GC. `cargo clean gc` is nightly, Cargo 1.88+ auto-cleans only `~/.cargo` caches, and `cargo-sweep` is unmaintained. Unpruned copies plus `unpacked` `.o` files reached 303 GB and 1M files in one `deps/` here, slowing every crate lookup.
- Reset with `cargo clean --workspace --profile dev` (1.93+). It drops workspace-crate copies while keeping dependencies compiled. In Octocode use `node skills-dev/octocode-dev/scripts/dev.mjs clean:cache`; full reset uses its `clean` task. Run cleanup only when requested or needed for a diagnosed build problem.
- `build.build-dir` (stable 1.91) moves intermediates out of `target/` but does not shrink them.
- Parallel agents or worktrees: share at most 3 `CARGO_TARGET_DIR`s in total, under a scratchpad, outside repo `target/` (Cargo's lock serializes builds in one dir; the same `--features` keeps units shared), and delete them when done. A solo build uses the warm workspace `target/`. In Octocode, do not commit local linker/sccache configuration.
- One integration-test binary per crate: `tests/main.rs` declares each `tests/*.rs` as a `mod`, with `autotests = false` and `[[test]] path = "tests/main.rs"`. Add a test asserting every file is declared. Measured: 31 → 6 executables and −80% bytes per build.

Next: workspace deps/lints/metadata → `references/workspace.md`; test tooling → `references/testing-and-tooling.md`.
