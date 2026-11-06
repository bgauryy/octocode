# Tree-sitter, graph, LSP, security and compact-view remediation

Updated: 2026-11-06. Scope: the Octocode monorepo and its bundled grammar consumers.
Owner of this work: local Codex agent. Cloud-agent identity, scope and delivery status: **unknown**.

## Coordination and handoff

The user authorized coordination with the other agent and remediation of all audit findings.
Codex task discovery failed twice (`Codex app tool request failed`). The existing
agents-communication database refused peer discovery because its schema fingerprint differs
from the installed runtime. Neither database nor peer state was changed. A separate temporary
coordination database records local leases only; it does not establish ownership against the
unreachable agent. Do not infer that messages were delivered.

Cloud agent: record your scope and claimed paths here before overlapping edits. Preserve
existing staged and unstaged changes. No commits, stash, publishing, cross-platform builds,
or broad cache cleanup are part of this task.

## Tasks

All tasks originate in the 2026-11-06 audit. Effort is unknown until measured. Checkboxes mean
verified completion, not merely an implementation or a passing process exit.

| ID | Priority | State / owner | What and where | Why / evidence | Acceptance and method |
|---|---|---|---|---|---|
| SEC-1 | P0 | Verified / local | Update `webpki-root-certs` in native `Cargo.lock` | 1.0.9 is affected by RUSTSEC-2026-0383; cross-platform `reqwest` dependency | Resolve a patched compatible version through Cargo; audit the lockfile; preserve unrelated lock changes; host build succeeds |
| SEC-2 | P1 | Verified / local | Correct credential detector test attribution in `engine/src/security/detector/tests/credential_values.rs` | Synthetic credential is redacted by `base64EncodedSecrets`, while test requires `envVarSecrets` | Test the public redaction invariant without pinning an overlapping generic rule; identifier values stay readable and credential values remain redacted |
| SEC-3 | P1 | Open / unassigned | Investigate harmless `localSearch.matchKeys` redaction | Cargo dependency paths appeared as `[REDACTED-ENVVARSECRETS]`; source content remained visible | Reproduce with harmless metadata; fix the producing/security boundary generally; real secret-shaped metadata remains protected |
| MIN-1 | P1 | Verified / local | Correct compact verification of empty C macro whitespace in `engine/src/minify/verify.rs` | Corpus sweep found one mismatch in Linux `lib/zlib_deflate/defutil.h`; reduced Clang preprocessing is unchanged | Failing regression first; compare grammar tokens without weakening literal/token-glue checks; original corpus file passes |
| MIN-2 | P1 | Verified / local | Make `engine/examples/compact_sweep.rs` a reliable verification gate | Sweep printed `changed 1` but exited zero; unread UTF-8 files were silently skipped | Nonzero exit on changes/panics; account for unread files and grammar/syntax fallbacks; success cannot imply all enumerated files were parsed |
| GRAPH-1 | P1 | Verified / local | Use file evidence for battery caller-file comparison in `skills-dev/octocode-graph-dev/scripts/graph-bench.mjs` | Java probe has 29 declarations; full caller records were unnecessarily paged for a file-set oracle | Request `view:files`, consume `row.file`, retain edge/site totals explicitly, preserve all continuations and recall/precision; focused runner tests and battery comparison pass |
| GRAPH-2 | P2 | Open / unassigned | Classify graph recall gaps with source and LSP | Text-oracle recall: JS .50, TSX .58, Scala .94, C#/Go .98; Assembly/C extra is a real C++ caller | For each actual miss, record binding evidence and unresolved reason before changing extraction/linking; no name-guess links or precision regression |
| STORE-1 | P1 | Verified / local | Explain continuation persistence failures in runtime cursor publication | Read-only default home caused generic `worker failed`; writable temporary home restored every graph page | Deterministic unwritable-store regression; actionable safe error on CLI/MCP, no silent memory fallback or lost continuation; owning core contract if needed |
| LSP-1 | P1 | Incomplete / local live matrix | Complete semantic LSP language matrix | Live matrix passed 80 CLI/MCP assertions for TS/JS/Python, then Go definition completeness/exact-location checks failed; process deadline ended the run before later languages or a final report | Rerun with LSP-2 fixes; diagnose Go from structured receipts; real definition/reference fixtures per available server, explicit unavailable prerequisites, no syntax-only semantic claims |
| ENV-1 | P2 | Blocked / host capability | Make special-mode filesystem test report unsupported host capability | Requested mode 4755 was observed as 0755, explaining the test failure | Inspect actual mode after setup; explicitly report unsupported setuid preservation; still test exact mode matching on capable hosts |
| ENV-2 | P1 | Blocked / host environment | Rerun network-dependent CLI/MCP suites | Acceptance (6 cases), classification retry and provider shapes fail on `listen EPERM 127.0.0.1`; transport 14/14 passed | Run unchanged suites where localhost listeners are allowed; retain blocked receipts until then; do not count environmental failures as tool defects |
| DEV-1 | P1 | Verified / local | Review automatic cleanup in `build:dev` task | Audit build invoked `clean:cache` automatically above 50 GiB and removed shared build artifacts | Ensure normal targeted host builds never implicitly clean another session's Cargo artifacts; explicit owner-driven cleanup only; test task plan |
| COORD-1 | P1 | Blocked / service owner unknown | Restore a compatible path to the cloud agent | Both available discovery routes failed | Discover exact peer identity, exchange scope and task IDs, obtain a correlated handoff; never alter incompatible shared state to force discovery |

## Baseline and validation

- Tree-sitter 0.27.1; 13 code grammar variants plus CSS/SCSS compact-only grammars.
- Engine: 1,465 passed, two failures (SEC-2 and ENV-1), five ignored.
- Runtime graph: 611 passed. CLI/MCP transport: 14 passed.
- Graph battery with writable temporary home: 13 repositories, 5,823 files, zero failed
  calls, every caller page complete. No graph implementation defect was established by the
  initial storage-denied run.
- Compact sweep: 420,051 files enumerated, 121,904 no-grammar fallbacks, 2,574 syntax
  fallbacks, one verifier mismatch. Invalid UTF-8 reads were not accounted for by the runner.
- All 15 grammar smoke cases passed. Full semantic coverage and cross-platform execution
  remain unverified; source-level grammar support is not semantic proof.
- Preserve the before/after battery and failing command receipts. Run focused checks before
  broader validation, rebuild native plus affected interfaces, then exercise fresh CLI/MCP.

## Completion record

### Verified changes

- **SEC-1:** Cargo resolved `webpki-root-certs` 1.0.11 with only that package's
  version/checksum changing. Cached RustSec audit exits 0 with zero known vulnerabilities;
  the database is dated November 5. Three unmaintained-package warnings remain (SEC-4).
- **SEC-2:** Preserve the test's redaction and reporting assertions without pinning an
  overlapping generic detector's precedence. The complete engine run now has 1,471 passing,
  one host-capability failure (ENV-1), and five ignored tests.
- **MIN-1:** Failing LF/CRLF empty-macro regression captured first. A separate token proof
  models preprocessor aggregates in either grouping direction while preserving interior
  literal bytes and rejecting lexical token glue. All 47 focused minify tests pass.
- **MIN-2:** An invalid UTF-8 fixture previously printed `changed 0` and exited 0. It now
  reports its exact unread path and exits 1. Missing directories, directory-entry failures,
  missing CLI roots and panics no longer imply successful verification. The full rerun
  enumerated 420,051 files, reported 5,067 unread files, and found **zero changed views**;
  the nonzero exit honestly marks its unread scope. Fallback counts remain 121,904
  no-grammar and 2,574 syntax-error cases.
- **GRAPH-1:** Regression failed before the file-view change. All 13 repository cases
  retain identical recall, precision, caller edge totals and caller file sets, with every
  page complete and zero failed calls. Java's selected caller output fell from
  10,028,291 to 476,947 bytes while retaining 24,864 edges and 274 files. These are
  representation sizes, not controlled timing measurements.
- **STORE-1:** Regression failed with generic `worker failed` before the fix. All 23
  cursor-store tests pass. Fresh CLI and MCP calls with an unusable store now report
  `executionFailed` with a safe, actionable `OCTOCODE_HOME`/permissions message and publish
  no continuation. No new public error code or silent storage fallback was introduced.
- **DEV-1:** Normal `build:dev` and Rust test selections no longer trigger size-based
  cleanup. Obsolete process-discovery/size helpers were removed after LSP reference
  verification found both callers. Explicit cleanup remains available. All 18 build-plan,
  skill-test discovery and native-publication tests pass.
- **DOC-1:** Current docs and checker now agree on 17 contract tools, 16 default CLI
  commands and 12 default MCP tools; classification adds its command. MCP's security
  switch does not hide the CLI command. Shared usage copies regenerated from their owner:
  348 shared files, zero stale. Documentation verification exits 0.
- **CHECK-1:** Extracted unchanged schema-type helpers from the 425-line validator;
  preserved the concurrent validation edits. Contract tests: 88 passed, two ignored.
  Source-size check: zero failures. Engine/runtime Clippy with `-D warnings` exits 0.
- **LSP-2:** Captured a failing regression where emitted indexing retries continued beyond
  the caller's deadline. Both emitted and same-query retries now respect the suite's own
  deadline and retain the last partial answer. All 20 readiness/oracle/project tests pass.
  Every semantic case supplies that deadline. Live reports checkpoint each assertion with
  an explicit running/finished state, so interruption does not erase completed evidence.
  The timed-out matrix had loaded the previous helper; its results do not validate this fix.
  A fresh `ts` live run passes all 10 checks (eight CLI/MCP calls), with zero failures or
  unavailable servers and a terminal `finished` checkpoint. The full matrix remains open.

### Additional tasks and remaining limits

| ID | Priority | State / owner | What and where | Why / evidence | Acceptance and method |
|---|---|---|---|---|---|
| DOC-1 | P1 | Verified / local | Catalog checks and stale MCP/shared usage counts | Concurrent CLI security exposure made checker expectations stale | `docs:verify` passes; canonical shared-copy check has zero stale files |
| CHECK-1 | P1 | Verified / local | Split schema type helpers by responsibility | Validator had 425 lines | Mechanical extraction; 88 contract tests pass; source-size check passes |
| SEC-4 | P2 | Open / dependency owner unknown | Review `zbus_names`, `zvariant`, `zvariant_derive` maintenance warnings | Cached audit reports three unmaintained packages, not known vulnerabilities | Identify platform/dependency owner, select a maintained compatible line, validate affected platform before upgrades |
| DATA-1 | P1 | Open / unassigned | Page search key-path evidence instead of budget omission | `local_search/enclosing/match_keys.rs` drops later maps with an omission note when `page_budget` is exceeded | Reproduce tiny windows; every selected key-path row remains reachable exactly once via executable continuations |
| CHECK-2 | P1 | Blocked / host environment | Finish full `skills:check` | 13 network-dependent failures reported, then no final summary; representative search and WebSocket tests independently fail `listen EPERM 127.0.0.1` | Rerun on a host permitting listeners; preserve failures; do not report the interrupted/incomplete suite as passing |
| COORD-2 | P2 | Open / communication package owner unknown | Keep CLI builds stable for active coordination clients | Heartbeat failed loading a missing `dist/chunk-*.js` during this task | Atomic build publication or a pinned runtime; interrupted heartbeat is surfaced and leases are reacquired before edits |
| LSP-2 | P1 | Focused tests verified / local; live rerun open | Bound indexing retries by the caller's existing deadline and checkpoint results | The old matrix exhausted its 20-minute external deadline without a final structured report | Regression fails before fix; 20 focused tests pass; a fresh live matrix must retain partial receipts and reach a terminal report |

SEC-3 remains reproduced on the rebuilt CLI: **22 harmless Cargo key-path rows** were
redacted as `ENVVARSECRETS`, on one complete response page. The structured sanitizer treats
its public field named `key` as a credential assignment. A coherent next fix is a core-owned
`keyPath` contract field with all native serializers, tests and consumers regenerated together;
no alias or broad security exception. Do not edit generated contracts or weaken arbitrary JSON
credential protection to hide this symptom. Cloud/core ownership and write routing remain unknown.
Unapplied review patches are retained in the receipts directory as `key-path-core.patch` and
`key-path-native.patch`; both patches pass `git apply --check` in their owning package. These are proposals,
not a migrated contract or a verified security fix.

ENV-1 is not fixed by silently skipping the test: this filesystem demonstrably changes
requested 4755 to 0755. Validate the exact-mode invariant on a capable host.

Coordination discovery still fails through both routes. The separate local session's heartbeat
also stopped after a coordination build chunk disappeared. It was resumed and path ownership
reacquired before the completion record was written. No cloud-agent delivery is claimed.

### Validation receipts

Expensive baseline/after results and command logs are retained under
`.octocode/octocode-dev/2026-11-06-grammar-remediation/`. In particular, `before.json` and
`after.json` reconstruct the graph comparison. Final host build succeeded. Fresh CLI/MCP
storage-error receipts passed. `docs:verify`, shared-copy validation, source-size checks and
focused suites pass. Full network-dependent checks, unavailable language servers, the
setuid-dependent filesystem test and contact with the cloud agent are not complete.

