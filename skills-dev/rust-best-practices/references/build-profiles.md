# Build profiles and compile cost

Load when changing profiles, investigating rebuilds, reducing compile time or diagnosing target-directory growth. Measure the command and artifact that users or CI actually run.

## Diagnose before changing flags

1. Record toolchain, host/target, package selection, features, profile, Cargo config and relevant environment. Separate a clean build from a warm build and an edit/relink cycle.
2. Use `cargo build --timings` for the selected graph. Inspect its critical path and linking cost; the heaviest dependency is not necessarily the bottleneck.
3. Inspect feature enablement with `cargo tree -e features -i <dependency>` and rebuild explanations with verbose Cargo output. Different selections/settings can retain different compiled artifacts.
4. Change one factor and repeat the same workload. Report time, memory/disk cost and runtime/binary effects relevant to the change.

Cargo reads manifest profiles from the workspace root; Cargo config and environment can override them. Do not edit a member profile and assume it took effect.

## Profile decisions

| Candidate | Benefit to test | Cost or constraint |
|---|---|---|
| Less dev debuginfo | Shorter compile/link and smaller artifacts | Reduced debugger detail; preserve the information needed for diagnosis. |
| Platform-appropriate split debuginfo | Different compile/link/debugger tradeoff | Validity and defaults vary by platform/toolchain. |
| Optimize a hot dependency in dev | Faster local execution | Slower dependency compilation and a different artifact configuration. |
| Thin/fat LTO | Cross-crate optimization | Link time, memory and runtime effects depend on workload. |
| Fewer codegen units | Optimization opportunities | Less parallel codegen; do not impose one value on every crate. |
| Size optimization or symbol stripping | Smaller shipped binary | Runtime/debuggability costs; keep symbols for profiling/crash analysis as required. |
| Alternative linker or compiler cache | Faster relevant build stages | Platform support, cacheability and actual hit rate; do not infer benefit from installation alone. |

Preserve panic strategy unless the task includes it. Test harnesses do not validate an ordinary `panic = "abort"` deployment; check the actual artifact/boundary when that behavior matters.

For runtime profiling, start from release-equivalent settings with usable symbols. If disabling LTO or changing optimization to aid a profiler, disclose the change and confirm the result on the deployed configuration. A profile name alone does not prove equivalence.

## Keep build storage under its owner

- Reuse the repository's target directory and established feature selections. Test/dev inheritance can share settings, but differing features, flags or targets still cause extra builds; inspect before duplicating overrides.
- Consider an opt-in development feature-unification scheme only after proving repeated compilation is material. Ensure it does not ship test stubs or alter release features.
- Nightly unit graphs and compiler flags are optional diagnostic tools, not prerequisites. Check availability without changing the project's pinned stable toolchain.
- Do not run `cargo clean`, delete a shared cache, relocate targets or overwrite linker/cache settings as a speculative fix. Diagnose stale artifacts and coordinate according to repository rules before cleanup.
- In Octocode, follow [local build ownership](octocode.md), which requires the shared target directory and host-only builds. Do not start a private build lane to evade a Cargo lock.

Sources: [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html), [Cargo build timings](https://doc.rust-lang.org/cargo/reference/timings.html), [Rust Performance Book: build configuration](https://nnethercote.github.io/perf-book/build-configuration.html).
