# Native runtime audit and fixes

Date: 2026-11-13. Scope: `packages/octocode-native` only. Mode: audit and fix.
Baseline revision: `dda2e20f27af1c75a3c9752b1e2432a4acccef6e`, with existing changes in the shared checkout.

## Executive summary

Five defects were reproduced in the native GitHub provider and shared HTTP retry code. Fixes preserve missing-versus-empty evidence, decode JSON blob responses, recognize submodule metadata, and remove two arithmetic overflow paths. No public schema, generated contract, dependency version, or coverage floor changed.

Confidence is high in the reproduced mechanisms and targeted fixes: all ten new regression tests pass in the fresh focused build. Full verification remains incomplete; this report does not establish that every native path is defect-free. The audit mapped all six crates, searched the production source, and inspected selected high-risk execution, evidence, network-policy, storage and lifecycle paths. It was not a manual line-by-line review of every file.

## Key findings

| Priority | Confirmed defect and trigger | Cause and impact | Fix and source |
|---|---|---|---|
| P1 | A JSON blob response is returned as file source. | `fetch_blob` treated every successful body as raw bytes. A server returning the JSON representation supplies a base64 wrapper instead of the selected file. This affects shared GitHub reads used by CLI and MCP. | Decode JSON media responses with the existing decoder; preserve raw responses and binary bytes; discard malformed retained answers. [`content/file.rs`](../../crates/github/src/content/file.rs). |
| P2 | Missing/null content, or an empty body for a nonempty file without a usable fallback, can become a successful empty file. | `unwrap_or_default` erased the distinction between absent and explicitly empty content. The fallback path could also fall through when no blob SHA existed. | Require present inline content, use blob acquisition when inline content is unavailable, and return a typed decode failure if acquisition is impossible. [`content/payload.rs`](../../crates/github/src/content/payload.rs), [`content/file.rs`](../../crates/github/src/content/file.rs). |
| P2 | Submodule metadata accompanying `type: "file"` is ignored. | Detection checked only the type string. A submodule URL is stronger evidence that the object belongs to another repository. Affected reads can lose the typed submodule diagnosis or omit the submodule result. | One payload predicate recognizes the type or the submodule URL; both file admission and submodule lookup use it. A null URL on a regular file remains valid. [`content/payload.rs`](../../crates/github/src/content/payload.rs), [`content/listing.rs`](../../crates/github/src/content/listing.rs). |
| P2 | An extreme year in a server's `Retry-After` date reaches unchecked calendar arithmetic. | The parser accepted arbitrary `i64` years. Debug tests panic in `days_from_civil`; unchecked release arithmetic can wrap. Process-wide termination was not demonstrated. The helper is shared by GitHub, registry and classification HTTP clients. | Validate the IMF-fixdate four-digit year before calendar arithmetic. This is the protocol's bound, not an invented operational quota. [`retry.rs`](../../crates/io/src/retry.rs). |
| P3 | Jitter panics when its millisecond ceiling saturates at `u64::MAX`. | The inclusive range length `ceiling + 1` overflows; release wrapping would produce a zero divisor. Existing ordinary retry policies use smaller durations; live reachability of the extreme value is unverified. | Use the random word directly when the inclusive range spans every `u64`; retain the existing calculation for smaller ranges. [`retry.rs`](../../crates/io/src/retry.rs). |

Each finding is owned by native. The triggers above are deterministic regression inputs, not claims that these malformed responses were observed from live GitHub. Two public GitHub API reads through the available web reader were unavailable, so live-provider confirmation remains open.

## Analysis and evidence

The design fixes the first divergent boundary: decode provider representations correctly, retain missing-data errors, and validate external numeric syntax before arithmetic. It adds no fallback service, dependency, configuration setting, handwritten public tool schema or guessed evidence cap.

Raw responses retain the shared byte buffer; JSON responses reuse the existing content decoder. Missing bodies remain explicit failures unless the provider supplies a blob reference that can recover them. The media-type boundary assumes the origin accurately labels its representation, as the existing contents reader already does. Replacing the date parser with a library was a viable broader option; validating its supported IMF-fixdate input fixes this overflow without adding a dependency or expanding supported formats.

Regression evidence before fixes:

- GitHub: 8 focused tests ran; 3 passed and 5 failed. Failures covered missing content, an unavailable nonempty file, submodule metadata, JSON blob decoding and malformed blob content.
- I/O: both new overflow regressions failed. The date case panicked in calendar arithmetic; the jitter case overflowed its inclusive range length.
- Counterexamples cover explicitly empty files, null submodule URLs, binary content, raw JSON files, and untyped raw bodies. JSON decoding also checks GitHub vendor media types, parameters and case-insensitive media type spelling.

Workspace run records are retained under `.octocode/native-audit-20261113/`: `regressions-before.log`, `io-before.log`, `focused-final.log`, and `verification-results.json`. They are local evidence rather than repository documentation dependencies. The fresh focused run includes the final media-type case expansion. The repository regression sources are [file decoding tests](../../crates/github/src/content/file/tests.rs), [payload tests](../../crates/github/src/content/payload/tests.rs), and [retry tests](../../crates/io/src/retry/tests.rs).

The earlier graph critical-file sampling and unequal-rank defects are already addressed in the current source. `detect_critical` iterates all qualifying files, and the shared PageRank code has tie handling; those historical issues were not re-reported as open defects.

### Verification

Commands below run from `packages/octocode-native` unless otherwise stated. `RUSTC_WRAPPER=` disables the optional sccache wrapper, which this sandbox refuses to run.

| Check | Result |
|---|---|
| Native packaging tests | Passed: 35 Node test-runner tests, plus platform detection and platform package consistency checks. |
| Native Vitest tests | Passed: 2 tests. |
| Native TypeScript typecheck | Passed: `yarn workspace @octocodeai/octocode-native typecheck`. |
| Native oxlint | Passed: `yarn workspace @octocodeai/octocode-native lint:oxlint`. |
| Dependency usage | Passed: `cargo shear --locked --deny-warnings --check-test-targets`. |
| Dependency policy | Passed offline against an isolated copy of the cached advisory database, revision `5c26bd59903e07edadefe5af8a7350e4c588e199` dated 2026-11-06. Advisories, bans, licenses and sources passed. This is not a current advisory refresh. |
| Contract freshness | Passed: `yarn workspace @octocodeai/config check:tool-contract`. |
| Documentation verification | Passed: `node skills-dev/octocode-dev/scripts/dev.mjs docs:verify` from the repository root. |
| Changed-file formatting | Passed: `rustfmt --edition 2024 --check` on the touched Rust modules and their child tests. |
| Whole-workspace formatting baseline | Failed before the production edits: formatting differences in 113 existing files. Unrelated files were not reformatted. |
| Initial Rust selection | Environment-blocked: 9 passed and 3 failed before fail-fast stopped the first selection. All three failures could not bind mock-server ports. |
| Initial engine selection | 666 passed, 1 failed before fail-fast stopped the selection. The failing setuid-mode test is explained by the sandbox changing requested mode `4755` to observed mode `0755`; a separate filesystem probe confirmed this. |
| Focused regressions after fixes | Fresh compile and nextest: 18 tests ran, 17 passed, 1 failed because the existing HTTP-body test cannot bind its mock server. All ten newly added regressions passed. Command: `RUSTC_WRAPPER= cargo nextest run --locked -p octocode-github -p octocode-io --lib --no-fail-fast -E 'test(content::file::tests) or test(content::payload::tests) or test(retry::tests)'`. |
| Full Rust selections with failure continuation | Attempted, but the long run ended without a test summary. No full-suite pass is claimed. |
| Clippy, all targets and features | Failed with `E0603`: the CLI schema code called a crate-private `tool_contract_named`. Another session exposed the function publicly while checks were running. The corrected tree still needs a successful full lint run. Workspace record: `clippy.log`. |
| Host build | Incomplete: queued behind the shared Cargo build lock. The task's queued verification process was stopped with Ctrl-C, exit 130, after approximately 20 minutes across the compiler-check/build cycle. No completed rebuilt binary is claimed. Workspace record: `host-build.log`. |
| Fresh CLI and stdio MCP smoke checks | Unverified against a rebuilt binary because the host build did not complete. The targeted provider regressions exercise the changed native decoding boundary. |

### Risks and limits

- Local port binding is denied. HTTP mock tests and socket-based integration checks cannot be treated as passed here.
- Several sessions share the Cargo target and source tree. Concurrent classification, pagination, graph, schema, runtime error and numeric retry-conversion edits were preserved. Check results apply to the recorded runs; they do not establish a clean result for subsequent edits to the shared checkout.
- No release, cross-platform build, performance benchmark, Miri run or fuzz campaign was executed. These changes introduce no unsafe code or dependency updates. Runtime performance improvement is not claimed.
- Security review included selected address-policy, path-policy, atomic-write, request-lifecycle and LSP partial-result paths. Passing checks do not establish the absence of all security defects.

## Recommendations

1. Require the new regressions to pass with the existing ordinary-size retry and raw-content cases. Keep malformed provider data as an explicit typed failure.
2. Re-run HTTP and socket integration coverage in an environment that permits local listeners. Success means the normal package gates pass without weakening the tests or ignoring unexplained failures.
3. Refresh the advisory database and run the existing dependency policy gate before release. The cached database result is dated and conditional.
4. Keep schema ownership, interface delegation and evidence acquisition in their current layers. No architecture rewrite is justified by these failures.

## Next steps and sources

Once the shared build lock is available, run `RUSTC_WRAPPER= yarn workspace @octocodeai/octocode-native verify` from the repository root in an environment that permits local mock-server listeners and normal filesystem modes. Rebuild with `yarn workspace @octocodeai/octocode-native build:dev`, then check configuration, schema discovery and the changed provider behavior through CLI and a fresh MCP process. Completion requires successful package gates and the same selected evidence on both interfaces, without weakening tests or coverage floors. Full production-readiness verification is not claimed by this audit. Publishing and commits remain owner-controlled.

Primary references used to check applicability:

- [GitHub Contents API for the transport's 2022-11-28 API version](https://docs.github.com/en/rest/repos/contents?apiVersion=2022-11-28): submodule metadata, directory type behavior, and unavailable inline content for larger files.
- [GitHub Git Blobs API](https://docs.github.com/en/rest/git/blobs#get-a-blob): JSON base64 and raw blob representations.
- [RFC 9110, date/time formats](https://www.rfc-editor.org/rfc/rfc9110.html#name-date-time-formats): IMF-fixdate's four-digit year.
- [Tokio task cancellation](https://docs.rs/tokio/latest/tokio/task/): cancellation requests and completed cleanup differ; used when reviewing existing lifecycle code, not as evidence for a new lifecycle defect.
- [cargo-deny advisory configuration](https://embarkstudios.github.io/cargo-deny/checks/advisories/cfg.html): isolated advisory database location for the sandboxed offline check.
