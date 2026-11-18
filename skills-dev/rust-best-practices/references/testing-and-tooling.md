# Tests, diagnostics and verification

Load for test design, compiler/lint checks, CI or selecting verification tools. Use the repository's task runner and installed tools first. Missing optional tooling is an unchecked scope item, not permission to install or replace the toolchain automatically. Numbers below come from one 9,238-test workspace ([provenance](sources-and-crates.md#refinement-provenance)).

## Choose checks by risk

| Change | Smallest meaningful check | Broaden when needed |
|---|---|---|
| Behavioral defect | Regression that fails before the fix and passes after it | Error paths and affected callers/interfaces. |
| Internal refactor | Existing focused behavioral tests and compile/lint check | Public API, lifetime or concurrency changes. |
| Public API or examples | Consumer/integration test and enabled doctests | SemVer, MSRV and supported feature/target matrix. |
| Parser or untrusted bytes | Boundary cases and invariants | Property tests or a relevant fuzz target. |
| Unsafe or synchronization invariant | Focused invariant/failure tests | Supported Miri, concurrency modeling and real platform/FFI tests. |
| Runtime/build performance | Reproducible baseline and representative benchmark | Profile the measured bottleneck; check resource costs. |
| Documentation-only | Links, structure and any changed executable examples | No Rust rebuild solely for prose. |

## Write trustworthy tests

- Assert observable behavior and invariants rather than mirroring implementation steps. Do not copy a tunable constant into the expected literal; assert the owning invariant.
- Keep tests out of implementation files: a test module is its own file (`foo/tests.rs`, mounted by `#[cfg(test)] mod tests;`), so a test edit recompiles its test unit, not the library and every crate above it. Where a test lives (a unit-test module, a test crate, one integration binary per crate, a support crate) is a layout decision: see [place tests](crate-structure.md#place-tests).
- Write many-case behavior as data: inputs and expected outputs in fixture files or inline snapshots, run through one checking helper. rust-analyzer's tests are mostly fixture-driven with [expect-test](https://docs.rs/expect-test/latest/expect_test/); ruff, uv, oxc and biome use [insta](https://insta.rs/), ruff adds `mdtest`, and Deno runs `__test__.jsonc` specs. Less test code compiles: literal `json!` alone was 41% of one lib-test unit's expanded code ([compile cost](code-size.md#macro-volume-in-tests)). An API change then touches the helper, not every test.
- Own every temporary resource in a guard: [`tempfile::TempDir`](https://docs.rs/tempfile/latest/tempfile/), the repository's helper, or a fixture whose `Drop` cleans up. A `remove_dir_all` at the end of a test never runs when an assertion fails first: one such fixture leaked 2,420 directories. Tests killed by a timeout before `Drop` left 1,186 more (432 MB). Child processes need the same ownership: start a daemon or server through a helper that stops and reaps it (five CLI tests leaked background daemons and sockets). To prove it, point `TMPDIR` at an empty directory per run and check that it is still empty afterwards. Keep live-network tests explicitly separate from hermetic tests.
- Keep test state out of process globals. A `static` override (an atomic limit, a global budget, a process-wide memo), environment variable or working directory that one test sets leaks into every test sharing the process. A global limit that one test set and never reset failed an unrelated test in the full run while it passed alone. A process-wide memo evicted another test's cursor in 2 of 15 runs. Pass the value in. If work on other threads must read a process-wide value, hold it through an RAII guard that lets one test at a time set it and restores the default on drop, also when an assertion fails. Follow edition safety requirements when manipulating the environment. With every test process-safe, libtest can be the gate (runner row below).
- Inject time. Clocks and timers are values the owner passes in, with the real clock in production and a manual clock that tests advance. Avoid sleeps, tokio's `test-util` paused clock and thread-local hooks behind a feature:
  - `test-util` is a dependency feature ([`tokio::time::pause`](https://docs.rs/tokio/latest/tokio/time/fn.pause.html)). Either the dev build unifies it, and tokio's `now()` then goes through its mutex-guarded test clock in every dev binary, or the dev and test graphs fork (20 units here).
  - A thread-local hook is read on the production path of every build that enables it, and it does not follow work onto another thread. Here a dev-only hook even made dev builds walk differently from release: they checked the clock at every entry instead of every 64th.

  An injected timer replaced 130 paused-clock uses, and the dev graph then held no `test-util` unit.
- Shrink heavy tests instead of feeding megabytes through unoptimized code. Make the threshold a test exercises injectable (a ceiling, window, page size or line bound), scoped to that test, and prove the same behavior on kilobyte inputs. Here 182 tests over 1 s took 672 of the largest binary's 1,026 s of test time. One graph test fed a 1.28 MB bundle (22.8 CPU s, 886 s wall run alone); it proves the same at 32 KB. If a profile puts the time in a dependency's hot loop, A/B that dependency's [dev opt-level](build-profiles.md#profile-decisions). Never weaken an assertion to save time.
- Search hot byte and substring paths with [`memchr::memmem`](https://docs.rs/memchr/latest/memchr/memmem/), or [aho-corasick](https://docs.rs/aho-corasick/latest/aho_corasick/) for many needles at once, rather than `str::contains`, `windows().any()` or `iter().position()`. Generic std search code is instantiated at opt-level 0 in dev builds and shared across crates: the profiled instance came from a dependency, so no profile override on the calling crate reaches it. `memmem` runs in memchr's own compiled code, which one `[profile.dev.package.memchr]` override optimizes. A `memmem` pass over 32 MB went from 25–70 ms to about 1 ms, and two heavy tests from 20.5 to 6.9 and from 16.9 to 6.4 CPU s.
- Replace a heavy test; do not keep its old form as an `#[ignore]` twin. The default run skips an ignored test ([`ignore`](https://doc.rust-lang.org/reference/attributes/testing.html#the-ignore-attribute) runs only with `--ignored` or `--include-ignored`), yet it compiles in every test build and its assertions rot unseen. Real scale belongs in a named end-to-end or benchmark suite that a job runs.
- Test cancellation deterministically. A cancelled operation must report that it was cancelled, wherever the cancel was first seen ([cancellation](concurrency-and-async.md#cancellation-and-completion-are-different)). Inject the cancel as a predicate that fires at its Nth poll, count the polls of an uncancelled run, then try every N. Each result must be whole or say it was cancelled. The existing timing-based test failed in 6 and 8 of 40 runs without naming a cause. The injected version failed deterministically on the defect (a cancel first seen inside a file returned a partial result with no cancelled reason) and passed 400 of 400 runs after the fix. Use the same injected point for any race: a barrier or a hook at the deciding step, not a sleep.
- Keep assertions and failure messages useful. Follow the repository's test lint exceptions; do not globally allow production unwraps or panic just to simplify tests.
- Turn a discovered crash or counterexample into a durable regression. Review each snapshot change before accepting it; never bless incorrect output to make a test pass.

## Run the right graph

Prefer existing task commands. Otherwise, substitute observed Cargo package names and supported features into these examples; `<package>` is a placeholder, not a shell command to run literally:

```bash
cargo check --locked -p <package> --all-targets
cargo test --locked -p <package> <test_filter>
cargo clippy --locked -p <package> --all-targets -- -D warnings
```

Use `-D warnings` only when it is the project's gate. Use the same package and feature selection for check, test and Clippy as the build: each other selection compiles another copy of the affected crates ([needless rebuilds](build-profiles.md#stop-needless-rebuilds)). Clippy already typechecks; do not immediately repeat an identical check without a reason. A successful check does not execute tests.

Format only the intended scope using the repository's formatter and effective edition/config. Check the resulting diff. Avoid formatting unrelated files or the whole shared workspace during another session's build.

At handoff, run applicable project gates once. Expand package, feature or target selection only for affected behavior or unresolved risk. `--all-features` is appropriate only for a supported combination. Test the actual MSRV/target when compatibility changed, or report it unverified.

## Optional tools, selected by need

| Need | Candidate and limitation |
|---|---|
| Test runner | libtest runs each test binary in one process, so per-process start-up (a first client, contract parsing, regex setup) is paid once per binary. [nextest](https://nexte.st/docs/design/why-process-per-test/) runs a process per test: it isolates process state and kills hung tests, but repeats that start-up for every test. Make tests process-safe (above) and gate with libtest: 238 s wall and 974 CPU s, against nextest's 315 s and 1,183 CPU s for the same 9,238 tests, with 0 failures in both libtest runs. Keep nextest for tests that spawn real child processes: on macOS a pipe is created without close-on-exec and marked a moment later, so a child another test spawns in that moment can inherit its end. libtest has no per-test timeout, so run it under a watchdog. nextest runs no doctests; run `cargo test --doc` for enabled ones. |
| Order of test binaries | Run binaries one after another, each with the build's thread share. Starting them all at once inside the same share oversubscribed it: 538 s and 1,389 CPU s, against 239 s and 974 CPU s sequentially. |
| CI time on a large workspace | Run only tests affected by the changed crates (nextest `rdeps()` filtersets, as Zed does) and partition across machines (`--partition`, as uv does). |
| Input invariants | Existing proptest/fuzz target; shrinking or fuzzing does not replace an explicit behavioral oracle. |
| Unsafe execution | Supported Miri tests; they do not prove soundness or cover arbitrary FFI/platform APIs. |
| Concurrency schedules | Loom or an established model harness; code must use the modeled primitives. |
| Public compatibility | Existing SemVer/MSRV checks; inspect baselines and supported scope. |
| Coverage or mutation quality | Existing coverage/mutation tool; preserve floors and investigate meaningful gaps, not just percentages. |

Read the selected tool's versioned help before copying flags. Do not run automated fixes, refresh baselines or bless snapshots as if they were read-only checks. A green command proves only its executed scope; report skips, unsupported platforms and missing components.

## Tools that keep code and architecture clean

Use the ones the repository already runs; propose others with the evidence that motivates them.

| Tool | What it enforces or shows | Use it when |
|---|---|---|
| Workspace lint table ([`[workspace.lints]`](https://doc.rust-lang.org/cargo/reference/workspaces.html#the-lints-table), `[lints] workspace = true` in each member) | One lint policy for every crate, for example `dead_code`, `unused_*` and `private_interfaces` denied, and Clippy's `unwrap_used`, `expect_used` and `allow_attributes_without_reason` | Always; a new crate opts in in the change that creates it |
| A Clippy ratchet: a baseline of existing findings per lint and crate that may only shrink | A strict lint applies to new code at once, without a big-bang fix of old code | Adding a lint to an existing tree; when files move between crates, re-key the baseline with per-lint totals unchanged |
| [cargo-deny](https://embarkstudios.github.io/cargo-deny/) | Advisories, licenses, banned and duplicate versions, allowed sources; each exception carries a reason | In CI and on every dependency change |
| [cargo-shear](https://github.com/Boshen/cargo-shear), or [cargo-machete](https://github.com/bnjbvr/cargo-machete) as a cross-check | Unused dependencies, including those only test targets declare (`--check-test-targets`); it caught an unused workspace dependency this round | After moves and removals. Both report candidates: check build scripts, macros, renamed and feature-gated uses before removing one. |
| A crate-edge guard test | Allowed workspace dependencies per crate | Any manifest change ([rule](crate-structure.md#clean-crate-architecture)) |
| [`cargo --timings`](https://doc.rust-lang.org/cargo/reference/timings.html) | Critical path, and frontend against codegen time per unit | Before changing layout or profiles, and after, to confirm |
| Nightly [`--unit-graph`](https://doc.rust-lang.org/cargo/reference/unstable.html#unit-graph) | Unit keys per command; a diff shows a dev/test feature fork | After changing features or dev-dependencies ([workspace](workspace.md#features-and-msrv)) |
| [cargo-llvm-lines](https://github.com/dtolnay/cargo-llvm-lines) | IR lines and copies per generic function | A codegen-heavy crate ([compile cost](code-size.md)) |
| [cargo-nextest](https://nexte.st/) | A process per test, slow-test kills, filtersets and partitions | Tests that spawn processes; CI selection and sharding |
| [bacon](https://dystroy.org/bacon/) | A watch loop running check, Clippy and tests | The local edit loop, with jobs that use the exact package and feature selection of the project's scripts so they reuse the same compiled units |
| [tempfile](https://docs.rs/tempfile/latest/tempfile/) | Temporary files and directories removed on drop | Every test that writes to disk |
| [memchr](https://docs.rs/memchr/latest/memchr/) and [aho-corasick](https://docs.rs/aho-corasick/latest/aho_corasick/) | Vectorized single- and multi-needle search in their own compiled code | Hot byte and substring scans, and many literal needles at once |

For Octocode's feature selections, lint ratchet, tool pins and CLI/MCP checks, read [Octocode development](octocode.md).

Sources: [Cargo test](https://doc.rust-lang.org/cargo/commands/cargo-test.html), [Clippy usage](https://doc.rust-lang.org/clippy/usage.html), [rustdoc tests](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html), [nextest running tests](https://nexte.st/docs/running/) (no doctests), [nextest filtersets](https://nexte.st/docs/filtersets/), [Miri](https://github.com/rust-lang/miri), [testing attributes](https://doc.rust-lang.org/reference/attributes/testing.html).
