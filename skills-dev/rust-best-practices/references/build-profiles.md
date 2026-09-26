# Build profiles & compile time

Load when tuning `[profile.*]`, making release binaries faster/smaller, or cutting dev/CI compile time. Why: profiles are cheap, high-leverage, and easy to get subtly wrong — and most slow builds are a critical-path problem, not a "slow crate" problem. Profiles take effect **only in the workspace-root** `Cargo.toml`.

## Release — the shipped artifact
```toml
[profile.release]
lto = "fat"           # cross-crate opt for what ships; "thin" only while iterating
codegen-units = 1     # max optimization on hot crates (see per-package override)
opt-level = 3         # "s"/"z" when size dominates (CLI, wasm, addons) — benchmark both
strip = "symbols"
panic = "abort"       # smaller/faster; pairs with denying unwrap/expect/panic
```
- `panic = "abort"` breaks `#[should_panic]`/`catch_unwind`; restore it for tests/coverage: `[profile.coverage] inherits = "release"` + `panic = "unwind"`.
- **Selective `codegen-units`:** `= 1` serializes a crate's codegen. Keep it on the crate with hot CPU loops; give orchestration/IO/glue crates `[profile.release.package.<crate>] codegen-units = 16`. Only that crate rebuilds; runtime perf is unchanged. "Hot" is a `--timings` + benchmark answer.
- Cohort (oxc, ruff, uv, deno): fat LTO for release; if a 6-platform matrix makes link time hurt, parallelize CI rather than ship thin.
- Published binaries get a named profile: `[profile.dist] inherits = "release"` (ruff/uv).

## Profiling profile — profile without editing release
```toml
[profile.profiling]      # ruff/uv name; oxc/deno: release-with-debug
inherits = "release"
debug = true             # line tables for samply/perf/flamegraph
strip = false
lto = false              # uv: fat-LTO compile times "completely untenable" here
```
`cargo build --profile profiling`.

## Optimize one hot dependency in dev/test
```toml
[profile.dev.package."regex-automata"]
opt-level = 3            # repeat under [profile.test.package."…"]
```
Do this for regex, compression, crypto, codecs that dominate runtime while your crates stay `opt-level = 0`.

## Dev-loop compile time
- Diagnose first: `cargo build --timings` → read the **critical path**, not the slowest crate. A heavy dep compiling in parallel off the serial tail buys ~0 when swapped (e.g. `aws-lc-sys` → `ring` saves nothing and can break reqwest's rustls provider). Attack your own members on the tail.
- Split god crates so an edit recompiles less (`references/crate-boundaries.md`); fewer generics/macros in hot-recompile crates.
- Faster linker (`mold`/`lld` via `.cargo/config.toml` `-C link-arg=-fuse-ld=…`) when linking dominates incremental rebuilds; Cranelift backend for dev codegen.
- `[profile.dev] debug = "line-tables-only"` (+ `split-debuginfo = "unpacked"` on macOS) shrinks link/debug-info time.
- **`sccache` is a CI/clean-build lever:** it caches dependency rustc calls, but not the `cdylib`/bin workspace crate you edit, and disables incremental — verify with `sccache --show-stats` before using it locally.
- Give CI cores: 2025+ rustc scales near-linearly to ~32.
- `target/` grows without GC (every feature × profile × toolchain): `cargo sweep --maxsize <GB>` keeps the warm set; `cargo clean` forces cold.

Next: for workspace-level deps/lints/metadata, load `references/workspace-manifest.md`; for runtime measurement, `references/performance.md`.
