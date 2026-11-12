# Tests, diagnostics and verification

Load for test design, compiler/lint checks, CI or selecting verification tools. Use the repository's task runner and installed tools first. Missing optional tooling is an unchecked scope item, not permission to install or replace the toolchain automatically.

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
- Unit tests may access private items; integration tests exercise the public API. Cargo normally makes each top-level integration-test file a target. If link/startup cost warrants consolidation, explicitly register modules and verify none disappeared.
- Use existing temporary-directory helpers for filesystem tests. Inject roots, clocks and randomness where practical; use synchronization or runtime time controls instead of sleep-based races. Keep live-network tests explicitly separate from hermetic tests.
- Process-global env/cwd changes can race concurrent tests. Prefer child-process configuration or injected values; follow edition safety requirements when manipulating the environment.
- Keep assertions and failure messages useful. Follow the repository's test lint exceptions; do not globally allow production unwraps or panic just to simplify tests.
- Turn a discovered crash or counterexample into a durable regression. Review snapshots and avoid blessing incorrect output to make a test pass.

## Run the right graph

Prefer existing task commands. Otherwise, substitute observed Cargo package names and supported features into these examples; `<package>` is a placeholder, not a shell command to run literally:

```bash
cargo check --locked -p <package> --all-targets
cargo test --locked -p <package> <test_filter>
cargo clippy --locked -p <package> --all-targets -- -D warnings
```

Use `-D warnings` only when it is the project's gate. Select the same required features for each command. Clippy already typechecks; do not immediately repeat an identical check without a reason. A successful check does not execute tests.

Format only the intended scope using the repository's formatter and effective edition/config. Check the resulting diff. Avoid formatting unrelated files or the whole shared workspace during another session's build.

At handoff, run applicable project gates once. Expand package, feature or target selection only for affected behavior or unresolved risk. `--all-features` is appropriate only for a supported combination. Test the actual MSRV/target when compatibility changed, or report it unverified.

## Optional tools, selected by need

| Need | Candidate and limitation |
|---|---|
| Faster test runner | Existing nextest setup; it does not run doctests. Run `cargo test --doc` for enabled doctests separately. |
| Input invariants | Existing proptest/fuzz target; shrinking or fuzzing does not replace an explicit behavioral oracle. |
| Unsafe execution | Supported Miri tests; they do not prove soundness or cover arbitrary FFI/platform APIs. |
| Concurrency schedules | Loom or an established model harness; code must use the modeled primitives. |
| Public compatibility | Existing SemVer/MSRV checks; inspect baselines and supported scope. |
| Dependencies | Existing advisory/license checks and unused-dependency scan; review generated/conditional uses before removal. |
| Coverage or mutation quality | Existing coverage/mutation tool; preserve floors and investigate meaningful gaps, not just percentages. |

Read the selected tool's versioned help before copying flags. Do not run automated fixes, refresh baselines or bless snapshots as if they were read-only checks. A green command proves only its executed scope; report skips, unsupported platforms and missing components.

For Octocode's feature selections, lint ratchet, tool pins and CLI/MCP checks, read [Octocode development](octocode.md).

Sources: [Cargo test](https://doc.rust-lang.org/cargo/commands/cargo-test.html), [Clippy usage](https://doc.rust-lang.org/clippy/usage.html), [rustdoc tests](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html), [nextest doctests](https://nexte.st/docs/running-tests/doctests/), [Miri](https://github.com/rust-lang/miri).
