# LSP AST and Graph Quality Audit

Date: 2026-11-13 (Asia/Jerusalem). Scope: native LSP, AST, graph storage and queries, CLI output references, and public graph skills. Existing unrelated work was preserved. No commits or releases were made.

## Executive summary

Confirmed evidence-loss, continuation, resource-retention and graph-discovery defects were corrected. AST live probes found no confirmed defect in the inspected paths. The host build, targeted regressions and fresh CLI/MCP parity checks passed. The runtime LSP suite passed 571 tests; 27 IDE-provider fixtures failed because the sandbox refused local socket binding. Broader Rust suites and remaining checks below limit a full production-readiness sign-off.

A final protocol-edge check confirmed that malformed pushed diagnostic notifications could establish a false clean report. Required-array validation and its regression are in progress; production verification must be repeated after that correction.

## Key findings

| Area | Confirmed cause | Implemented correction | Evidence |
|---|---|---|---|
| LSP diagnostics | Push storage discarded report suffixes after 2,000 diagnostics or 256 KiB, before runtime pagination. Rerunning could not recover an unchanged discarded suffix. | Retain complete reports through the existing shared memory budget. Evict whole reports and disclose unavailable evidence. Materialize reports outside the memo lock. | [Push storage and regressions](../../crates/engine/src/lsp/transport/push_diagnostics.rs) |
| LSP diagnostic deduplication | Each unique diagnostic scanned every previously retained payload. | Hash into candidate buckets, then compare exact values. Preserve first occurrence order and every location, without cloning payloads into keys. This is an algorithmic improvement; no latency claim is made. | [Diagnostic operations](../../crates/runtime/src/tools/lsp_search/ops/diagnostics.rs) · [Regressions](../../crates/runtime/src/tools/lsp_search/tests/diagnostics.rs) |
| Graph discovery | Published manifests above 1 MiB became undiscoverable despite successful publication. | Stream metadata for discovery, selection and retention; read complete manifests only where full evidence is needed. | [Snapshot discovery](../../crates/runtime/src/tools/ast_graph/store/snapshots.rs) · [Regressions](../../crates/runtime/src/tools/ast_graph/store/tests/snapshots.rs) |
| Graph recovery | Lexical snapshot comparison missed successors across numeric suffix boundaries such as `-9` to `-10`. | Use the existing numeric snapshot ordering. | [Snapshot discovery](../../crates/runtime/src/tools/ast_graph/store/snapshots.rs) |
| Graph compression | A compressed encoded body could fit publication while exceeding the reader's expanded-byte limit. | Enforce the existing snapshot bound before compression in both blocked and unblocked writers. | [Compression admission](../../crates/runtime/src/tools/ast_graph/store/format/blocks.rs) · [Publication](../../crates/runtime/src/tools/ast_graph/store/format/writer/publication.rs) |
| Graph continuation budgeting | Internal request reconstruction omitted `symbolName`. | Preserve the filter when calculating and publishing continuation requests. | [Paging](../../crates/runtime/src/tools/ast_graph/store/query/paging.rs) · [Regression](../../crates/runtime/src/tools/ast_graph/store/query/evidence_list_tests.rs) |
| Graph skill consumers | Fragment joins duplicated nested arrays, lost text, accepted missing parts and discarded later complete pages with the same query index. Generic recovery could mix old and restarted snapshots. | Join declared collections and text pointers, keep repeated context once, retain every logical slice, and allow completed fragment groups to restart on later tool pages. Reject incomplete or cyclic walks and failed responses before following recovery. | [Shared consumer](../../../../skills/octocode-graph-view/scripts/tools.mjs) · [Graph reader](../../../../skills/octocode-graph-view/scripts/graph-pages.mjs) · [Tests](../../../../skills/octocode-graph-view/scripts/tools.test.mjs) |
| LSP graph crosschecks | Batched probes missed response pages or treated partial nonempty results as resolved. | Exhaust response pages before assigning answers; preserve unanswered reasons and reject missing continuations. | [LSP consumer](../../../../skills/octocode-graph-view/scripts/lsp.mjs) · [Tests](../../../../skills/octocode-graph-view/scripts/lsp.test.mjs) |
| Graph build receipts | A paused build or first progress slice could be mistaken for the published snapshot. | Follow the exact expansion continuation and select the final completed receipt. | [Shared build consumer and tests](../../../../skills/octocode-graph-view/scripts/tools.test.mjs) |
| Partial graph coverage | Section projection discarded partial metadata; whole-analysis briefs could hide incomplete chunks. | Propagate partial reasons and missing chunks through section snapshots, combined coverage and both briefs. | [Coverage](../../../../skills/octocode-graph-view/scripts/coverage.mjs) · [Regression](../../../../skills/octocode-graph-view/scripts/partial.test.mjs) |
| CLI reconstruction metadata | Installed CLI clients could not obtain the authoritative collection routes needed to distinguish evidence from repeated context. | Expose embedded `collectionPaths` and optional `collectionRefs` through the output reference, shared with MCP. No schema copy or generated-contract edit. | [Output reference](../../crates/cli/src/cli/schema/output.rs) · [Contract parity test](../../crates/cli/src/cli/schema/tests/output.rs) · [Data contract](../../../../docs/TOOL_DATA_CONTRACT.md) |

## Analysis and verification

### Live probes

These calls used the available installed/development tools during investigation. They demonstrate the observed behavior of those binaries, rather than substituting for a final rebuilt-interface check.

| Surface | Probe | Observed result |
|---|---|---|
| AST symbols | `limit:1`, Unicode declarations | Nine top-level declarations plus one nested method reconstructed exactly once across nine calls. |
| AST match | `limit:1` | Eight unique declarations reconstructed across eight calls. |
| AST syntax tree | Named and anonymous nodes, provenance, `responseChars:1500` | All 168 nodes across 168 response pages matched the ordered larger-window baseline. |
| AST malformed source | Recovered TypeScript parse | Explicit `parseRecovered` and `isPartial`. |
| AST Unicode scope | Mid-surrogate UTF-16 boundary | Explicit `invalidInput`. |
| AST mutations | Changed declaration inventory and changed match source | `staleSnapshot`; comment-only symbol changes retained the unchanged selected inventory. |
| LSP references | Rust symbol, small row pages | Nine references across five files over three pages; no observed gap or duplicate. A separate caller check returned retryable `ContentModified`, so that identity check remains unverified. |
| Graph row pagination | Whole result versus emitted one-row continuations | Exact ordered reconstruction for find (9), symbols (5), witnesses (23), unresolved (3), dependents (3), callers (3), callees (2), hubs (1), impact (4). |
| Graph cycle members | 60-file import cycle | All 60 unique members reached through nested continuations, without gaps or duplicates. |

### Executed checks

| Check | Result |
|---|---|
| Integrated graph-view Node tests: tools, graph-pages, LSP, partial coverage, algorithms, query, refresh, stitch, wiki | Final rerun passed: 74 tests, zero failures, exit 0. |
| Skill review across `skills` and `skills-dev` | Passed: zero errors and warnings. |
| Targeted Rust formatting | Passed for 32 changed Rust files. |
| `git diff --check` | Passed. |
| Native focused LSP tests | `cargo dev-test --lib -- tools::lsp_search`: 571 passed, 27 failed, exit 101. Every failure was an IDE-provider fixture's local socket bind returning EPERM. All new diagnostic regressions passed. |
| Native graph regressions | All five new storage/pagination regressions passed against the compiled runtime test binary. The broader 466-test storage run was interrupted under heavy host load; it is incomplete. |
| Native AST changed-path checks | All six symbol-row representation/provenance checks passed. The broader 358-test AST run was interrupted under heavy host load; it is incomplete. Live AST pagination and mutation probes are listed above. |
| Engine retention and JSON accounting | Passed: 12 focused engine tests plus one matching runtime test, zero failures. Includes complete 4,097-item reports, large Unicode diagnostics, whole-report eviction, oversized replacement and heap accounting. |
| CLI schema and MCP listing unit tests | Passed: 26 tests, zero failures. Shared instruction checks use one test-support module; output references match embedded collection metadata for every tool. |
| Host native build | Passed: `node scripts/build-native.cjs`, current Mac ARM64 only, host binary staged. No release or cross-platform build. |
| Final rebuilt-binary CLI/MCP smoke | Nine checks passed across 27 CLI and 24 fresh stdio MCP calls. Six Unicode/provenance AST matches reconstructed through 18 tiny pages per interface; two cycles and all 53 members through five raw graph pages per interface. |
| All-tool output metadata | All 17 CLI output references matched the authoritative contract's collection paths and recursive references exactly. |
| Documentation verification | Final rerun passed, including this report. An intermediate failure from another audit document's links was resolved in the shared tree. |
| Full skill script gate | Incomplete: baseline included socket-dependent failures under sandbox restrictions and exceeded the bounded run. Focused graph checks passed separately. |
| Repository-wide file-length check | Failed on pre-existing violations across the tree; changed CLI schema modules were reduced below 400 lines, and the changed stitch module is 399 lines. |

The compiled runtime test binary used for these native checks had SHA-256 `c99eb3f96c6333b84f939d6219dcdf26169b31ee020cbd6bf8de6de69aa3dfd2`.

The final rebuilt host binary used for interface checks had SHA-256 `bb433b33ce0589b079cd635ca067736fad5fccbfef7301f94d518b0ce950df14`. The fresh MCP process was closed and its exit awaited; temporary fixtures and isolated state were removed.

Prior graph concerns were also checked against current source: critical-file detection has no top-2-percent or ten-file candidate cap; PageRank ties share one percentile; impact rows require inspection and do not establish breakage. Existing regressions cover eleven qualifying hubs and equal impact/scores for tied dependencies. Stale native documentation claiming maximum row/depth limits was corrected against the current contract.

### Execution dependencies and ownership

| Task | Owner | Dependencies | State |
|---|---|---|---|
| LSP implementation and regressions | LSP audit agent with engine protocol reviewer | Discovery and contracts | Focused engine and runtime regressions passed; IDE socket fixtures blocked |
| AST audit | AST audit agent | Discovery and contracts | Live probes complete; no confirmed AST defect |
| Graph implementation and regressions | Graph audit agent with storage reviewer | Discovery and contracts | All five new native regressions passed |
| Graph skills and consumers | Graph skill agent | Actual CLI/native contract | Node checks passed |
| Independent adversarial review | Separate AST and skill reviewers | Implemented changes | Found additional memory and fragment issues; corrections applied |
| Integrated build and interface checks | Coordinator | All implementation lanes | Host build and final-source CLI/MCP checks passed; one Cargo owner, shared target directory |

## Recommendations and next steps

Finish the lint check, then rerun the interrupted broad native suites on a host with sufficient resources and local socket permissions. Preserve exact continuation inputs and compare reconstructed evidence, including tiny response budgets, consecutive split groups and partial results. The final host build, CLI schema/MCP listing unit tests and pinned CLI/fresh-MCP checks have passed.

Graph rows remain syntax candidates. Import cycles, direct impact, PageRank, source-site counts and Git co-change do not establish runtime breakage or probabilities. The public graph instructions now make those boundaries and selected traversal depth explicit.

## Risks and uncertainty

- Confidence is high in the reproduced root causes, focused native regressions and Node fixes; broader sign-off remains conditional on the uncompleted checks above.
- This was not an exhaustive language/server matrix or production-scale memory/performance campaign. Dynamic dispatch, reflection and runtime registration require source and execution evidence beyond the syntax graph.
- Full selected manifests are still materialized for reuse and freshness. Extreme selected-manifest memory stress is pre-existing and untested; metadata discovery no longer repeatedly materializes scan evidence.
- The compression regression uses the existing configurable writer seam. The default 512 MiB boundary was not allocated for a live stress test.
- AST syntax-tree response pagination was verified. Tool-level syntax-tree mutation continuation was not exercised because the fixture occupied one tool page.
- Process inventory and host memory-pressure inspection were denied by the sandbox. Known launched command sessions and agent fixtures were tracked individually.
- The observed Node runtime was 26.4.0; the repository declares `^24.15.0`. Compatibility on the declared Node 24 line was not separately executed.

## Primary references

- [LSP 3.17 specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/) and [pinned publish-diagnostics source](https://github.com/microsoft/language-server-protocol/blob/ba1f1947af19dbf3c33fc763eadd7e7848f80766/_specifications/lsp/3.17/language/publishDiagnostics.md).
- [ast-grep rule reference](https://ast-grep.github.io/reference/rule.html) and [Tree-sitter basic parsing](https://tree-sitter.github.io/tree-sitter/using-parsers/2-basic-parsing.html). The locked Tree-sitter version is 0.27.1.
- [serde_json 1.0.154 `from_reader`](https://docs.rs/serde_json/1.0.154/serde_json/fn.from_reader.html), matching the lockfile, and [Serde ignored values](https://serde.rs/ignored-any.html).
- [RFC 6901 JSON Pointer](https://www.rfc-editor.org/rfc/rfc6901).
