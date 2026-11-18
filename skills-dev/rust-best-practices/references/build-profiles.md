# Build profiles and compile cost

Load when changing profiles, investigating rebuilds, reducing compile time or diagnosing target-directory growth. Measure the command and artifact that users or CI actually run. Times below are CPU seconds (user + sys) unless marked wall, from one 690k-line workspace on 12 cores with rustc 1.99.1 ([provenance](sources-and-crates.md#refinement-provenance)); they justify the rule, not a universal speedup.

## Diagnose before changing flags

1. Record toolchain, host/target, package selection, features, profile, Cargo config and relevant environment. Separate a clean build from a warm build and an edit/relink cycle.
2. Run `cargo build --timings` (`cargo test --no-run --timings` for tests) on the selected graph. Read the critical path, each unit's frontend and codegen times, and linking. A unit whose frontend time dominates is limited by its size, derives or generics, which no profile flag fixes: see [size signals](crate-structure.md#size-signals) and [compile cost](code-size.md). The heaviest dependency is not necessarily the bottleneck.
3. Count what recompiled. `cargo build -v` names why each unit is dirty, and `--message-format=json` marks each artifact fresh or not. A repeated build with no edit must recompile nothing; fix that before tuning anything.
4. Compare CPU time (user + sys) or recompiled unit counts, not wall time, whenever other builds run. Here one crate needing 80 CPU s took 365 s of wall time at load 59–172. Alternate the variants (A, B, A, B), change one factor at a time, and report time, memory/disk cost and runtime/binary effects relevant to the change.
5. Re-measure a kept setting after a structural change. A proc-macro gain of 10–23% measured before a generated-type dedupe, partly at load 66, was gone after the dedupe (see [rejected here](#measured-and-rejected-here)).

Cargo reads manifest profiles from the workspace root; Cargo config and environment can override them. Do not edit a member profile and assume it took effect.

## Stop needless rebuilds

| Cause | Fix and check |
|---|---|
| A `cargo:rerun-if-changed` path that does not exist | Cargo treats a missing path as changed, so the build script, its crate and every dependent rebuild on every invocation. Here, back-to-back builds with no edit recompiled the runtime and CLI for 11–27 s each (`Dirty …: the file crates/io/build is missing`); after the fix, 1 s and 0 units. Emit only paths that exist; a second identical build must recompile 0 units. |
| A build script that hashes or watches a whole source tree | A directory watch reruns on any file in it ([build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html#rerun-if-changed)), so a unit-test edit rebuilds the library and all its dependents. Here 240k of the 455k hashed lines were test modules, and a unit-test edit rebuilt the library and the CLI. After hashing and watching, file by file, only what the non-test build compiles (skip `#[cfg(test)]` module subtrees; keep the `#[path]` and `include*!` targets that non-test code names), it recompiled 1 unit. |
| A different package or feature selection per command | Build, test and lint with the same selection, or each compiles another copy of the affected crates. Here one missing dev-dependency feature made each switch between the dev build and the tests recompile 27 units, every library included, instead of 5 test units; one shared test selection cut distinct units from 573 to 482. See [workspace](workspace.md#features-and-msrv) for the unit-graph proof. |

## Profile decisions

| Candidate | Measured or sourced effect | Cost or constraint |
|---|---|---|
| Less dev debuginfo (`debug = "line-tables-only"`, none for dependencies) | Shorter compile/link and smaller artifacts | Reduced debugger detail; keep a profile with full debuginfo for stepping. |
| No debuginfo for each large workspace crate (`[profile.dev.package.<crate>] debug = false`) | On the largest workspace crate: dev lib 65.6 and 75.5 → 52.7 and 54.0 CPU s (−24%), lib-test 171.9 and 164.4 → 144.3 and 139.0 (−16%). Give every crate split out of it the same setting. | Panic messages keep `file:line`; backtrace frames of that crate lose it. A package override applies to dev and test alike, so it forks no units (unlike `[profile.test]`). |
| No test debuginfo (`CARGO_PROFILE_TEST_DEBUG=0` in CI, or `[profile.test] debug = false`) | Smaller objects and faster links of large test binaries; biome, Bevy and Wasmtime CI set the variable, oxc sets the profile. Not measured here. | `cargo test` builds every unit, dependencies and library units included, under the `test` profile, so any `[profile.test]` setting that differs from dev forks every crate between `cargo build` and `cargo test`. Use it in a CI job that shares no target with dev builds, or where dev and test builds share no units anyway. Backtraces lose line numbers; panic messages keep `file:line`. |
| `incremental = false` in dev | Since rustc 1.88, incremental builds with `split-debuginfo = "unpacked"` leave a new set of `.rcgu.o` objects on every rebuild, and nothing removes them ([rust-lang/rust#161824](https://github.com/rust-lang/rust/issues/161824)). Here 731k of 737k `target/debug/deps` entries were such objects (307 GiB). | No incremental reuse of workspace crates. With incremental off, Cargo's codegen-units default is 16, not 256 ([profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#codegen-units)). |
| Platform split debuginfo | Faster links (no `dsymutil` or packing step) | Validity and defaults vary by platform and toolchain; see the incremental row. |
| Codegen units, per crate | 256 units on the largest crate, half of whose time was codegen: CPU unchanged (71.2 and 69.3 → 67.4 and 69.9 s), wall 43.7 and 41.4 → 39.2 and 39.8 s. On the engine crate: +9% CPU (16.8 and 16.5 → 18.3 and 18.0 s). In release, `codegen-units = 1` made one orchestration crate's serial codegen about 107 s of a 154 s critical path, so it gets 16; on the engine, 8 or 16 units slowed a minify benchmark by 17–22%, so it keeps 1. | Set it per crate only where an A/B shows a gain. Re-measure after a split shrinks the crate. More units do not help a frontend-bound crate. |
| Optimize a dependency in dev (`[profile.dev.package.<dep>] opt-level = 3`) | Only where a profile shows the dev loop or tests running in that dependency's code. Measured wins here: the regex stack (a first search over a 7 MB line 5.4–5.7 s, against 0.52 s in a release-like build; sanitizer setup 50 → 17 ms); serde_json (a differential test 110 → 70 CPU s); memchr (a `memmem` pass over 32 MB 25–70 ms → about 1 ms; see [testing](testing-and-tooling.md#write-trustworthy-tests)). | Slower cold compile of that dependency, on the cold critical path. Leave large generated C parsers (here about 74 MB of `parser.c`) at opt-level 0: `cc` follows the profile, so `-O3` costs minutes of cold build while tests parse tiny fixtures. |
| Thin/fat LTO | Cross-crate optimization | Link time, memory and runtime effects depend on workload. |
| Size optimization or symbol stripping | Smaller shipped binary | Runtime/debuggability costs; keep symbols for profiling/crash analysis as required. |
| Faster linker | Shorter links of large binaries | Platform support; measure the link step in `--timings`. |

Nightly-only levers (`-Zthreads`, `-Zshare-generics`, the Cranelift backend, checksum freshness) are not options for a project pinned to stable; do not change its toolchain for them. A nightly toolchain is fine for one-off diagnosis, such as `--unit-graph`, `-Ztime-passes` or `-Zprint-type-sizes`, without changing the pin.

Preserve panic strategy unless the task includes it. Test harnesses do not validate an ordinary `panic = "abort"` deployment; check the actual artifact/boundary when that behavior matters.

For runtime profiling, start from release-equivalent settings with usable symbols. If disabling LTO or changing optimization to aid a profiler, disclose the change and confirm the result on the deployed configuration. A profile name alone does not prove equivalence.

## Measured and rejected here

Do not adopt these without a new measurement that reverses the evidence.

| Practice | Evidence against it | Where it can still pay |
|---|---|---|
| Proc-macro stack (`proc-macro2`, `quote`, `syn`, derive crates) at opt-level 3 in dev | Applied, then reverted. After the generated-type dedupe, the derive-heavy crate's expansion fell from 2.37 to 1.55 s, but its CPU stayed at 20.2–20.6 s at opt-level 0, 1 and 3. Meanwhile the stack's cold compile rose from about 7 to 65.3 CPU s, on the cold critical path: about +58 s per cold build. | Zed sets opt-level 3 and oxc sets its macro crates to 1. It pays only when `-Ztime-passes` shows expansion as a large share of a hot crate after the derive volume is cut ([compile cost](code-size.md#generated-code)). |
| The same in release, or `build-override` opt-level 3 | Release: expansion −0.8 s, cold build +50 CPU s. `build-override`: the build-script run went from 1.0 to 0.37 CPU s, but 62 host units recompile on every cold build. | Rarely; measure cold and warm builds together. |
| One `codegen-units` value for every crate | 256 units saved wall time on the runtime and cost the engine 9% CPU (table above). | Per crate, by A/B. |
| sccache for the local edit loop | 2.8% hit rate for Rust with a 10 GB cache. Workspace path crates and test binaries are never cached; [corrode](https://corrode.dev/blog/tips-for-faster-rust-compile-times/) found the benefit "rather negligible". | Dependency builds shared across CI machines, adopted on a measured hit rate. |
| A workspace-hack crate (cargo-hakari) | Materialize measured it slowing builds and removed it, as did Zed ([workspace](workspace.md#features-and-msrv) cites both). | Remove the feature fork at its cause instead. |
| A private copy and target directory per task, built concurrently | Five concurrent builds, each in its own copy, on 12 cores and 32 GB reached load 59–172 with 140 MB free and heavy swap (below). | One shared build plus focused tests. |

## Share one build

- Reuse the repository's target directory and established package and feature selections. Test/dev inheritance shares settings, but differing features, flags or targets still compile extra copies; inspect before duplicating overrides.
- Run at most two Cargo builds on one machine at a time, and give each a share of the cores (`CARGO_BUILD_JOBS` and test threads of cores ÷ builds). One large crate's rustc held 4.4 GB, and its lib-test held 6–7 GB. With five concurrent builds on 12 cores, one crate took 365 s of wall time for 80 s of CPU; the cap stayed at two after swap reached 6.6 of 8 GB during this round. Oversubscription outweighs every compiler setting.
- Do not run `cargo clean`, delete a shared cache, relocate targets or overwrite linker/cache settings as a speculative fix. Diagnose stale artifacts and coordinate according to repository rules before cleanup.
- In Octocode, follow [local build ownership](octocode.md#development-and-checks): the build-slot wrapper, the shared target directory and host-only builds. Do not start a private build lane to evade a Cargo lock.

Sources: [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html), [Cargo build timings](https://doc.rust-lang.org/cargo/reference/timings.html), [build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html), [Rust Performance Book: build configuration](https://nnethercote.github.io/perf-book/build-configuration.html), [oxc](https://github.com/oxc-project/oxc/blob/main/Cargo.toml) and [Zed](https://github.com/zed-industries/zed/blob/main/Cargo.toml) manifests.
