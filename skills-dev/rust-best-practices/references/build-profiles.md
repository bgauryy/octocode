# Build profiles & compile time

Load when tuning `[profile.*]`, making release binaries faster/smaller, or cutting dev/CI compile time. Profiles take effect **only in the workspace-root** `Cargo.toml`.

## Release
```toml
[profile.release]
lto = "fat"           # cross-crate opt for what ships; "thin" only while iterating
codegen-units = 1     # max optimization on hot crates (see per-package override)
opt-level = 3         # "s"/"z" when size dominates (CLI, wasm, addons) — benchmark both
strip = "symbols"
panic = "abort"       # smaller/faster; pairs with denying unwrap/expect/panic
```
- `panic = "abort"` breaks `#[should_panic]`/`catch_unwind`. For tests/coverage: `[profile.coverage] inherits = "release"` + `panic = "unwind"`.
- `codegen-units = 1` serializes a crate's codegen. Keep it on hot-CPU crates; give IO/glue crates `[profile.release.package.<crate>] codegen-units = 16`. Runtime perf is unchanged. Find "hot" with `--timings` + a benchmark.
- Cohort (oxc, ruff, uv, deno) ships fat LTO. If a 6-platform matrix makes link time hurt, parallelize CI; do not ship thin.
- Published binaries: `[profile.dist] inherits = "release"` (ruff/uv).

## Profiling profile
```toml
[profile.profiling]      # ruff/uv name; oxc/deno: release-with-debug
inherits = "release"
debug = true             # line tables for samply/perf/flamegraph
strip = false
lto = false              # uv: fat-LTO compile times "completely untenable" here
```
Build with `cargo build --profile profiling`.

## One hot dependency in dev/test
```toml
[profile.dev.package."regex-automata"]
opt-level = 3            # repeat under [profile.test.package."…"]
```
Use for regex, compression, crypto, or codecs that dominate runtime; your crates stay `opt-level = 0`.

## Dev-loop compile time
- Diagnose with `cargo build --timings`; read the **critical path**, not the slowest crate. Swapping a heavy dep that builds off the serial tail saves ~0 (`aws-lc-sys` → `ring` saves nothing and can break reqwest's rustls provider). Attack your own members on the tail.
- Split god crates (`references/crate-structure.md`); fewer generics/macros in hot-recompile crates.
- Linking dominates incremental rebuilds → `mold`/`lld` via `.cargo/config.toml` `-C link-arg=-fuse-ld=…`; Cranelift for dev codegen.
- `[profile.dev] debug = "line-tables-only"` (+ `split-debuginfo = "unpacked"` on macOS) cuts link/debug-info time.
- `sccache` is a CI/clean-build lever: it skips the `cdylib`/bin crate you edit and disables incremental. Check `sccache --show-stats` before local use.
- CI cores: 2025+ rustc scales near-linearly to ~32.
- `target/` has no GC: `cargo sweep --maxsize <GB>` keeps the warm set; `cargo clean` forces cold.

Next: workspace deps/lints/metadata → `references/workspace.md`.
