# Claude handoff follow-up — 20.2.0

**The rebuilt release passes the selected all-17-tool acceptance checks, regression suites and source-identity checks.** The host release is `20.2.0`, SHA-256 `4e2052ae4edcbe0b5ef527b2ceaf4906b1c35bed52a3e4715da03bd555f24f35`. The [source guard](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/source-guard-final.json) verifies all 3,246 recorded source fingerprints without drift.

Claude transferred integration, builds and tests to Codex at a safe checkpoint. The [worker handoff](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/claude-handoff.md) and [correlated reply](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/communication-receipts.json) record actual handling, not only transport queuing. Sol implemented relevance changes; Astra independently reviewed the source and evidence. Root integrated changes and owns full validation.

## Changes and decisions

| Item | Outcome | Evidence |
|---|---|---|
| Response-cache eviction | Retain original repair notes and row-budget reserve privately with response-page work. Preserve original indices, page cuts and evidence without repeating rejected rows. | [Red finding](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/replay/FINDING.md), [review](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/replay/REVIEW.md) |
| Inline Rust test hits | Prioritize files with production hits and prefer production hits when selecting a content page within a file. Preserve counts and declaration identity; reuse request-local test-code facts. File grouping remains intact, and matchOnly does not gain intrafile reordering. | [Implementation and limits](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/relevance/REPORT.md), [independent review](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/relevance/INDEPENDENT-REVIEW.md) |
| Request repairs | Disclose actual envelope/sort repairs; valid defaults remain silent and removed enum names remain rejected. | [Decisions and regression evidence](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/repairs/decisions.md) |
| Core consistency | Correct two stale totalLines fixtures and the graph-site description, then regenerate the contract. | [58 tests in one targeted core file](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/core-tests.log), [regeneration](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/contracts-regen.json) |
| More caching | Add no cross-request module cache: existing request-local memoization remains the owner, avoiding a new invalidation contract. | [Design](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/relevance/DESIGN.md) |
| Repeated graph paths | Keep flat JSON/MCP rows. Grouping can shrink large answers but increases small pages; retain a measured renderer-only prototype as a future option. | [Measured alternatives](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/review/REPORT.md) |

The first full run caught a types-to-search dependency violation. The shared rank algebra was moved down to the type layer; the architecture rule and behavior tests were unchanged. [First failed run](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/rust-green-full.json) is preserved, including the explicitly interrupted engine phase. It is not combined with later results to claim a green run.

## Prior comparative ratings

These are the completed comparison scores attached to binary `c4f7ee4e8b49188d2a74d3c5d8a274e01c12cad64677a18a72b48104a18ff1c0`. Fresh acceptance does not automatically re-rate a new executable or repeat the rg/ast-grep/gh/Graphify/direct-LSP benchmarks. See the [full scorecard and rubric](../20.2.0-2026-11-14-tool-ratings/SCORECARD.md) for correctness, schema, workflow, efficiency, freshness, pros, cons and per-dimension provenance. Scores are out of ten; * means provisional coverage.

| Tool | Overall | Output | Pagination |
|---|---:|---:|---:|
| localSearch | 8.8 | 8 | 10 |
| localFetch | 8.7 | 8 | 10 |
| structureSearch | 8.8 | 8 | 10 |
| astSearch | 8.4 | 8 | 10 |
| clasify | 8.6* | 8 | 10 |
| lspSearch | 8.1* | 8 | 9 |
| graphBuild | 8.5 | 8 | 9 |
| graphQuery | 8.3 | 8 | 9 |
| ghSearchRepo | 8.1* | 8 | 9 |
| ghSearchCode | 8.5* | 8 | 10 |
| ghStructure | 8.8* | 8 | 10 |
| ghGetFileContent | 9.1 | 9 | 10 |
| ghSearchHistory | 8.2* | 8 | 9 |
| ghGetHistoryItem | 8.7* | 8 | 10 |
| ghCloneRepo | 9.0 | 8 | 10 |
| artifactSearch | 8.2 | 8 | 9 |
| securityCheck | 8.5* | 8 | 9 |

## Scope and limits

The follow-up [questions](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/README.md) and [all-tool preparation](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/all-tools/PREPARATION.md) own this round's checks. The prior 120-question inventory is not represented as 120 executed passes. Three timing samples on a six-file fixture establish observations, not general speedups or cache hits. Positive Rust-project LSP, representative unread-corpus classification efficiency, mutable provider-cache invalidation and newly dedicated classified long-line/multiline ranking combinations remain unverified unless a final receipt explicitly closes them. Graph grouping is a prototype, not a shipped improvement.

## Rebuilt validation

| Check | Result | Receipt |
|---|---|---|
| Full Rust rerun | 6,998 runtime/native + 1,665 engine tests passed; 16 skipped, zero failures | [Full rerun](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/rust-green-full2.json) |
| Rust lint | Passed; baseline unchanged | [Lint](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/rust-lint.json) |
| Config and core | All 162 config tests passed; 58 tests in the one changed core test file passed | [Config](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/config-tests.json), [targeted core](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/core-tests.json) |
| Build and contract | Contract regenerated, config built, host release built, output checks passed | [Build/source provenance](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/build-provenance.json), [output check](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/check-outputs.json) |
| Replay and repairs | 168 actual Node 24 CLI/MCP calls; retained/evicted/delayed page results and warnings agree; true source changes refused | [Live results](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/replay/live-after.json) |
| Evidence reconstruction | 18 page chains match independently reconstructed source exactly: 120 hits, 120 numbered lines, and 7,017 raw long-line characters; rejected rows appear once | [Source union proof](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/replay/source-union-proof.json) |
| All 17 tools | 34 positive cases + 34 invalid-input cases; 50 complete selected chains / 465 page calls; all 17 selected CLI/MCP result comparisons pass | [All-tool report](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/all-tools/REPORT.md) |
| Cleanup | Root's 172 recorded process IDs absent; Claude worker preserved. Rust pruning removed 364.7 MiB; 485 GiB free | [Root processes](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/root-process-cleanup.json), [Rust cleanup](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/rust-cleanup.json) |

The actual Node 24 CLI child, fresh MCP children and three owned warm backends all resolve to the exact official executable hash, as recorded in [runtime provenance](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/all-tools/native-executable-provenance.json). All 17 effective input schemas and output field references agree with generated presentation. The LSP description adds the detected server inventory at runtime; its canonical remainder agrees, and CLI/MCP effective descriptions match. Isolated configuration exposes 16 tools; the existing configured profile exposes all 17, including clasify. No credential copy was needed.

Selected evidence includes all 72 selected commits across 36 history pages per interface, 58 code-search hits, 613 Unicode lines, a complete 27,009-byte read and the full selected advisory affected/reference evidence. The selected history patch matches logical lines after gutter removal; the served text omits the oracle’s terminal newline, so this is not byte equality. Two small classification calls used 620 input and 80 output tokens; that does not establish representative classification efficiency. See [source proof](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/all-tools/source-proof-github-providers.json) and [classifier usage](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/all-tools/classifier-usage.json).

The long-line read uses raw source character pieces with source-range metadata. Ordinary line reads use numbered lines. Reconstruction compares each selected representation correctly; it does not assume the character pieces repeat line-number prefixes.

## Timing and optimization outcome

The six-file Rust fixture returns exactly the same ten hit facts and character counts in all 24 calls per build. Each arm has one warmup and three measured samples. These are shared-host observations, with OS/process warmness and no proven cache-hit attribution. The timing request explicitly uses ascending relevance (weakest first); the separate descending control confirms production files precede the file containing only inline test hits.

| Interface | Sort | Before median ms | After median ms |
|---|---|---:|---:|
| Native CLI | Relevance | 14.519 | 14.579 |
| Native CLI | Path | 14.391 | 14.458 |
| Node CLI | Relevance | 61.537 | 62.145 |
| Node CLI | Path | 60.941 | 60.130 |
| Direct MCP | Relevance | 8.757 | 7.338 |
| Direct MCP | Path | 7.173 | 7.358 |

[Timing receipts and limits](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/relevance/timing-comparison.json) preserve the values. Both Node timing arms use v26.4.0; acceptance uses the supported Node v24.22.2. The after run uses a short owned socket namespace, unlike the inherited before-run TMPDIR. Before-run launcher resolution was checked after measurement, not by observing that measurement's child. These limits preclude a general or causal speedup claim.

The shipped efficiency work reuses request-local classification and multiline offsets, avoids extra reads for path sorting and unmatched files, and retains only private page-shaping state needed for exact replay. No cross-request module cache was added. A lossless graph renderer prototype reduced one retained answer from 4,115 to 2,828 characters (31.28%), but JSON grouping increased existing tiny pages; neither prototype is shipped. The [prior optimization plan](../20.2.0-2026-11-14-tool-ratings/OPTIMIZATIONS.md) and [comparative scorecard](../20.2.0-2026-11-14-tool-ratings/SCORECARD.md) retain the rg, ast-grep, gh/lean-gh, Graphify and direct-LSP results.

## Final acceptance

The [independent review](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/final-review.md) and [completion receipt](../../../campaigns/20.2.0-2026-11-14-tool-ratings/handoff-followup/completion.json) record final acceptance and cleanup. Documentation verification and diff checks are recorded in the completion receipt. No claim is made that every schema combination or all 120 inventory questions was executed; response-cache replay was exercised on localSearch and localFetch, not exhaustively on every tool.
