Load when tuning `[profile.*]`, cutting dev/CI compile time, or when `target/` balloons or a step rebuilds what another step just built. Why: profile, feature and package choices affect compiled copies retained until cleanup. Profiles take effect **only in the workspace-root** `Cargo.toml`.

# Build profiles, compile time, and target/

## Release
```toml
[profile.release]
lto = "thin"          # cross-crate optimization with lower link cost; benchmark fat as an alternative
codegen-units = 1     # candidate for hot crates; benchmark against more units
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

Numbers below are historical observations, not expected gains on a new machine or workload. Reproduce the baseline before using them to justify a change.

- Diagnose with `cargo build --timings`; read the **critical path**, not the slowest crate. A heavy dep off the serial tail saves ~0 (`aws-lc-sys` → `ring` saves nothing). Attack your own members on the tail.
- Linker: lld is the Linux default since 1.90, and mold or wild is optional there. On macOS keep Apple ld: measured relink 1.2–1.6 s Apple vs 1.4–1.8 s lld, no win.
- `sccache` skips incremental (own-crate) compiles: "Non-cacheable: incremental". Its value is deps and C/C++ (tree-sitter, aws-lc) across clean builds, new `CARGO_TARGET_DIR`s, and CI. Check `sccache --show-stats` before you claim a win.
- Cranelift and `-Zthreads` are nightly only.

## One build, no copies: share units across steps
A unit's hash covers package, target, profile settings, features, and its deps' hashes. Every distinct combination is a separate compile and a separate copy on disk. Copies come from:
- **Different `-p` selections or `--all-features`.** Feature selection and unification depend on each Cargo command and the workspace resolver.
- **Dev-dependency features.** These can be active for targets that include dev dependencies. For example, an HTTP test dependency can enable additional hyper features, changing the compiled units shared with a dev build.
- **Test-only features** and `RUSTFLAGS`/env differences.

Measure; don't guess:
- `cargo +nightly build --unit-graph -Z unstable-options`, the same for `test --no-run`, then diff unit keys with the profile name excluded. This is deterministic and needs no compile.
- `cargo tree -e features -i <crate>` per selection shows who turns a feature on.
- `cargo build -v` prints `Dirty <crate>: <reason>` for unexpected rebuilds.

Fix, in order:
1. Give the edit loop one package and feature selection: the dev build, the test command, and `cargo check` on that crate. Clippy on that same selection replaces the check when you want lints early; do not run both. A workspace `--all-features` Clippy is the handoff gate and a second copy of the unit graph. Commands are in `references/testing-and-tooling.md`. Merge test commands that differ only in packages.
2. Unify dev-dep features through an opt-in feature that dev builds and tests enable and release never does:
   ```toml
   [features]
   dev-unify = ["dep:hyper", "tokio-util/codec"]   # mirror what dev-deps enable
   [dependencies]
   hyper = { version = "1", features = ["full"], optional = true }
   ```
   This approach is useful only if the unit-graph comparison shows a meaningful rebuild. Verify release features and binary size before adopting it; never enable a test stub in a shipped artifact.

## target/ hygiene
- Stable Cargo has no `target/` GC. `cargo clean gc` is nightly, and Cargo's automatic global-cache cleanup does not clean workspace `target/`. Unused compiled units and split-debuginfo objects can accumulate there; measure disk and inode use before cleanup.
- Reset with `cargo clean --workspace --profile dev` (1.93+). It drops workspace-crate copies while keeping dependencies compiled. In Octocode use `node skills-dev/octocode-dev/scripts/dev.mjs clean:cache`; full reset uses its `clean` task. Run cleanup only when requested or needed for a diagnosed build problem.
- `build.build-dir` (stable 1.91) moves intermediates out of `target/` but does not shrink them.
- Concurrent worktrees can use separate `CARGO_TARGET_DIR`s when Cargo's directory lock blocks progress. Bound their total disk and memory use, and clean scratch targets when done. A solo build can use the warm workspace `target/`. In Octocode, do not commit local linker/sccache configuration.

Next: workspace deps/lints/metadata → `references/workspace.md`; test tooling → `references/testing-and-tooling.md`.
