# Build profiles and compile time

Each kept setting in `Cargo.toml` and `.cargo/config.toml` carries its measurement in a comment beside it: read it before changing the setting, and put a new A/B's numbers there. Figures below are CPU seconds (user + sys) from 2026-11-17, rustc 1.99.1, a 12-core M2 Pro, unless stated.

## Measure

- Compare CPU time or recompiled-unit counts (`node skills-dev/octocode-dev/scripts/rust-quality.mjs units [-- <cargo build args>]` prints `rebuilt` and each rebuilt unit; `--file <cargo.jsonl>` reads a saved `--message-format json` log), never wall time: builds share the machine, and one crate's 80 CPU s took 365 s wall at load 59–172. Alternate A/B/A/B and change one factor.
- `cargo build --timings` (`cargo test --no-run --timings`) shows the critical path and frontend against codegen time per unit. A frontend-bound unit needs less code (below) or a [split](crate-structure.md), not profile flags.
- `$DEV llvm-lines <package>` counts IR per generic instantiation. Nightly `-Ztime-passes`, `-Zprint-type-sizes` and `--unit-graph` are fine for a one-off diagnosis; the pin stays stable, so nightly-only levers (`-Zthreads`, `-Zshare-generics`, Cranelift) are not options.
- A second build with no edit must recompile 0 units: `rust-quality.mjs units` reports `rebuilt: 0`.

## Rebuild causes found here

| Cause | Fix |
|---|---|
| `cargo:rerun-if-changed` naming a missing path | Cargo treats it as changed: runtime and CLI rebuilt on every build (11–27 s). Emit only existing paths. |
| A build script that watches a directory or hashes test code | `crates/runtime/build/` hashes and watches, file by file, only what the non-test build compiles, skipping `#[cfg(test)]` modules. Keep it so: a unit-test edit then recompiles 1 unit. |
| Build and tests selecting different features | One missing `dev-unify` entry made each switch recompile 27 units, every library included. Prove sharing by diffing the unit keys of nightly `cargo build --unit-graph -Z unstable-options` and the same `cargo test` selection: 0 dev units missing from the test graph. `rust-quality.mjs unit-diff <build.json> <test.json>` lists the keys only one has (exit 1 when any differ). |
| A `[profile.test]` setting that differs from dev | `cargo test` builds every unit, dependencies included, under `test`, so it forks them all. Per-package dev overrides apply to both. |

## Measured and rejected

Do not reintroduce these without a new measurement that reverses the evidence.

| Setting | Evidence |
|---|---|
| Proc-macro stack (`syn`, `quote`, derives) at opt-level 3 in dev or release, or `build-override` opt-level 3 | After the generated-type dedupe the derive-heavy crate stayed at 20.2–20.6 CPU s at opt-level 0, 1 and 3, while the stack's cold compile rose from about 7 to 65 CPU s. `build-override` recompiles 62 host units on every cold build. |
| One `codegen-units` value for every crate | 256 units: equal CPU on the facade, +9% on the engine. Set it per crate by A/B; the engine keeps release `codegen-units = 1` (8 or 16 slowed CSS minify 17–22%). |
| sccache for the edit loop | 2.8% Rust hit rate with a 10 GB cache; path crates and test binaries are never cached. |
| A workspace-hack crate (cargo-hakari) | Materialize and Zed removed theirs as slower. Remove the feature fork at its cause. |

## Cheaper code and tests (measured here)

| Pattern | Change | Effect |
|---|---|---|
| Generic memo or cache instantiated for many types | Non-generic core over `Arc<dyn Any + Send + Sync>` behind a thin typed façade | −212k IR lines; largest crate −3.2% CPU |
| Recursive walker generic over its callback | Take `&mut dyn FnMut`; keep the generic signature as an adapter | −52k and −22k IR lines |
| `async fn` wrapper that awaits a future it is given | Plain `fn` returning the future; `Box::pin` the inner operation once | Auth future 21,432 → 2,288 bytes |
| `BTreeMap` whose order nothing reads or serializes | `HashMap` | −14.7k IR lines |
| Many `sort_by*` closures | `Ord` on the row type, or shared named comparators | Not applied: 194 sort instances, 408k IR lines |
| Generated types | Dedupe identical shapes in the generator; derive `Deserialize` only where an input reaches | File 78k → 38k lines, crate 68 → 32 CPU s; 732 → 387 derives, −41% CPU |
| Literal `json!` in tests | A raw JSON string parsed by one `#[track_caller]` helper, or a fixture file; table-driven cases | 9,595 `json!` calls were 41% of the largest lib-test's expanded code (CPU effect unmeasured) |
| Hot byte or substring scans | `memchr::memmem` or aho-corasick, not `str::contains` or `windows().any()`: std's generic search compiles at the caller's opt-level 0, memchr's own code at its dev opt-level 3 | 32 MB pass 25–70 ms → about 1 ms; two heavy tests 20.5 → 6.9 and 16.9 → 6.4 CPU s |
| Slow tests on megabyte inputs | Inject the threshold the test exercises and prove the behavior on kilobytes; no `#[ignore]` twin | 182 tests over 1 s took 672 of the largest binary's 1,026 s |
