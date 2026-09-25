# Build & dependencies — profiles, compile time, supply chain

Load when tuning `cargo` profiles, cutting compile time, or auditing features/versions/supply chain. Why: the release profile and dependency hygiene are cheap, high-leverage, and easy to get subtly wrong.

## Release profile — the gold-standard template
```toml
[profile.release]
lto = "fat"           # whole-program / cross-crate opt for the SHIPPED binary
codegen-units = 1     # max optimization (slower compile, faster + smaller binary)
opt-level = 3         # or "s"/"z" when binary size dominates (CLI, wasm)
strip = "symbols"     # drop symbols
panic = "abort"       # smaller/faster if you never catch_unwind (see caveat below)
```
Profile settings **only take effect from the workspace root** — a member/dependency's `[profile.*]` is ignored. So these belong in the top-level `Cargo.toml`.

Cohort calibration (oxc, ruff, uv, deno all do this):
- **`lto = "fat"` for the shipped release; relegate `"thin"` to dev/profiling.** Fat costs link time you pay once per release target; thin recovers most of it and is the right default *only* while iterating. If a 6-platform matrix makes fat link time hurt, that's a CI-parallelism problem, not a reason to ship thin.
- **`panic = "abort"`** pairs with denying `panic!`/`unwrap`/`expect` — no unwind tables, smaller binary, and "let it crash" forces safe code (oxc's stated rationale). Caveat: it breaks `#[should_panic]` and `catch_unwind`, so override it back in a test/coverage profile: `[profile.coverage] inherits = "release"; panic = "unwind"`.
- **Selective `codegen-units` — often the biggest self-inflicted release-build cost.** Blanket `codegen-units = 1` maximizes runtime perf but *serializes* a crate's codegen. Two equivalent framings: ruff starts at the default 16 and drops to 1 only on hot crates; or, if your baseline is `1` everywhere, bump the NON-hot crates *up* to 16. Per-package overrides rebuild only that crate, not its deps:
  ```toml
  [profile.release.package.my_io_or_glue_crate]   # orchestration/IO, not a tight loop
  codegen-units = 16
  ```
  Keep `codegen-units = 1` only where it earns it — the crate with the hot CPU loops. Bumping only the non-hot crates leaves the hot crate's codegen unchanged, so it costs no runtime perf. Which crate is "hot" is a `--timings` + benchmark question, not a guess.

## A profiling / benchmarking profile (nearly universal in the cohort)
Fat-LTO + `strip` makes flamegraphs and `criterion` runs unusable, and rebuilding without them at `--release` is slow. Add a dedicated profile so you can profile without editing the release one:
```toml
[profile.profiling]        # ruff/uv name; oxc/deno call it `release-with-debug`
inherits = "release"
debug = true               # full line tables for perf/flamegraph/samply
strip = false
lto = false                # uv's Cargo.toml calls fat-LTO compile times "completely untenable" vs a "massive improvement" from lto=false
```
Build with `cargo build --profile profiling`. For a cargo-dist-published binary, add `[profile.dist] inherits = "release"` (ruff/uv) so the shipped artifact has one named, stable profile.

## Speed up a slow *dependency* without slowing dev builds
Bump optimization for one hot dependency even in dev/test, so debug builds aren't crippled by an unoptimized inner loop:
```toml
[profile.dev.package."regex-automata"]
opt-level = 3
[profile.test.package."regex-automata"]
opt-level = 3
```
Generalize: profile-override the specific crates that dominate runtime (regex engines, compression, crypto, image codecs) while your own crates stay at `opt-level = 0` for fast rebuilds.

## Cut compile time (dev loop)
- Default dev is already `opt-level = 0`, `codegen-units = 256` — parallelism over quality; keep it.
- **Faster linker:** `mold` (Linux) or `lld` — linking often dominates incremental rebuilds. Set in `.cargo/config.toml` via `-C link-arg=-fuse-ld=...`.
- **Cranelift backend** for dev codegen (faster than LLVM at `opt-level=0`); **`sccache`** to cache across branches/CI.
- Fewer generics/macros in hot-recompile crates; split god crates so a change recompiles less.
- Diagnose with `cargo build --timings` (HTML critical-path report), and read the **critical path, not the slowest crate**. A heavy dependency that compiles in *parallel* and finishes before your own crates start is NOT a build-speed target — optimizing or swapping it buys ~0 wall-clock. Example: swapping `aws-lc-sys` (via reqwest's rustls) for `ring` saves nothing when it compiles in parallel off the serial tail, and it risks breaking HTTPS (reqwest hardcodes the aws-lc provider; `ring` needs a manually installed global provider). Attack the crates on the serial tail instead — usually your own workspace members. 2025+ rustc scales near-linearly to ~32 cores — give CI the cores.
- **`sccache` caches at the wrong layer for workspace members.** It caches dependency-crate rustc invocations (the oxc/tree-sitter/reqwest bulk) — a big win for clean builds, branch switches, and CI cache-miss reruns (`RUSTC_WRAPPER=sccache`, incremental off). But it does *not* cache the final `cdylib`/binary workspace crate you actually edit (`Compile requests executed: 0` for those), and it disables incremental — so it can slow the tight single-file edit loop. Verify with `sccache --show-stats` before assuming a local win; it's primarily a CI/clean-build lever.
- **Reclaim `target/` — it grows unbounded.** Every distinct feature-set × profile × toolchain generation accumulates its own fingerprinted + incremental artifacts and is never GC'd; a heavy multi-feature workspace can reach *hundreds of GB* of mostly-stale debug output. `cargo clean` nukes it (forces a cold rebuild); **`cargo sweep --maxsize <GB>`** (or `--time <days>` / `--installed`) trims to the recent working set while keeping builds warm. Run it periodically or in a cron/CI step.

## Feature & version hygiene
- Add heavy optional deps with `default-features = false` and opt into only what you use — smaller graph, faster builds, less attack surface.
- Pin an **MSRV** (`rust-version`) and test it in CI; commit `Cargo.lock` for binaries/apps, leave it uncommitted for libraries, never hand-edit it.

## Workspace centralization — the three tables the cohort converged on
oxc, ruff, uv, biome, rolldown, rspack, and deno all centralize at the root and inherit in members. Missing any of the three is a drift hazard, not a style choice:

```toml
# root Cargo.toml
[workspace.package]                 # shared metadata — inherit in members
edition = "2024"                    # ONE edition for the whole workspace (all 7 cohort repos = 2024)
rust-version = "1.89"               # single MSRV floor, not re-pinned per crate
license = "MIT"
repository = "https://github.com/org/repo"

[workspace.dependencies]            # single source of truth for versions + baseline features
napi = { version = "3", default-features = false, features = ["napi4", "tokio_rt", "serde-json"] }
serde = { version = "1", features = ["derive"] }
tokio = { version = "1" }           # version here, per-crate features added on top

[workspace.lints.rust]              # define the lint policy ONCE
unused_imports = "deny"
[workspace.lints.clippy]
all = { level = "deny", priority = -1 }   # priority < 0 enables a group; override individuals above it
unwrap_used = "deny"
```
Members then inherit:
```toml
[package]
edition.workspace = true
rust-version.workspace = true
license.workspace = true
[dependencies]
serde = { workspace = true }                                  # exact inherit
tokio = { workspace = true, features = ["fs", "signal"] }     # add crate-specific features
napi = { workspace = true, optional = true }                  # `optional` is per-crate, not in workspace.deps
[lints]
workspace = true
```
Why each matters:
- **`[workspace.dependencies]`** — the real payoff is not DRY, it's preventing *version skew on a shared ABI*. Two crates that both link one N-API addon must compile the same `napi`/`napi-derive`; independent pins are a silent breakage. `optional` stays in the member (`{ workspace = true, optional = true }`); features are additive on top of the baseline.
- **`[workspace.lints]`** — a duplicated lint block *will* drift (one crate gains a lint, the other silently doesn't). Note the exclusivity rule: a member table using `workspace = true` **cannot** also add its own lints. So if one crate needs a stricter superset (e.g. a pure library denying `print_stdout` that the CLI crate can't), that crate keeps its own inline `[lints]` block *or* adds the extras as crate-root `#![deny(...)]` attributes — it can't do `workspace = true` plus extras in the manifest.
- **`[workspace.package]` + single `edition`** — mixing `edition = "2021"` and `"2024"` across one workspace is an anomaly no major project has; it means different macro hygiene / lifetime-capture / temporary-scope rules between crates that call each other. Centralizing edition structurally prevents the mismatch. Note edition 2024 has real breaking changes — migrate a lagging crate with `cargo fix --edition` and verify, don't just flip the string.
- `version` can also live in `[workspace.package]`, but only if your release tooling reads it there — regex-based version scripts that rewrite a literal per-crate `version = "…"` line break when it becomes `version.workspace = true`. Teach the tooling first, or keep version per-crate.
- A higher pinned `rust-toolchain.toml` channel **above** a lower `rust-version` MSRV floor is the norm (oxc/ruff/uv all do it) — the toolchain is what CI builds with; MSRV is the compatibility promise.

## Supply chain & dependency audit
| Tool | Catches |
|------|---------|
| `cargo audit` | Known RUSTSEC advisories in the tree |
| `cargo deny` | Banned/duplicate crates, disallowed licenses, advisory gate |
| `cargo machete` / `cargo udeps` | Declared-but-unused dependencies |
| `cargo tree -d` | Duplicate versions bloating the build |
| `cargo vet` / `cargo crev` | Human trust review of dependencies |
| `cargo outdated` + Dependabot/Renovate | Staleness / update cadence |
Gate `cargo fmt --check`, `cargo clippy -D warnings`, `cargo audit`, and `cargo deny check` in CI.

Adding/bumping deps, changing MSRV/edition, or touching the lockfile needs consent (SKILL lobby).

Next: for choosing the deps themselves, load `references/crates.md`; for the runtime cost of a dependency in a hot path, `references/performance.md`.
