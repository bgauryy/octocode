# Tool ratings — 20.2.0, 2026-11-14

**All 17 tools passed the named final CLI/MCP correctness checks after repair and rebuild.** The final pin is `c4f7ee4e8b49188d2a74d3c5d8a274e01c12cad64677a18a72b48104a18ff1c0`. Scores below apply to the measured scopes; unknowns remain provisional. Earlier measurements retain their original pin, and the formerly lossy security timing is excluded.

This round defines 120 owned questions across all 17 tools, with shared CLI/MCP checks and critical 1–10 ratings. A question file is a runnable behavioral specification, not a claim that every subcase passed. [Questions](../../../questions/tools/README.md), [common checks](../../../CHECKS.md), [rubric](../../../RATINGS.md) and [machine-readable ratings](ratings.json) own the details.

## Ratings and limitations

C = correctness (30%); S = schema/input (15%); O = output (15%); P = pagination (10%); W = workflow (10%); E = efficiency (15%); F = cache/freshness (5%). Each dimension is out of 10. Unknown is not a zero or an exemption; important unknowns make the overall score provisional. These scores are fresh judgments, not doubled historical 1–5 scores.

| Tool | C | S | O | P | W | E | F | Overall |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| localSearch | 10 | 9 | 8 | 10 | 9 | 6 | 9 | **8.8** |
| localFetch | 10 | 9 | 8 | 10 | 9 | 5 | 9 | **8.7** |
| structureSearch | 10 | 9 | 8 | 10 | 9 | 6 | 9 | **8.8** |
| astSearch | 9 | 9 | 8 | 10 | 9 | 5 | 9 | **8.4** |
| clasify | 9 | 8 | 8 | 10 | 8 | — | 9 | **8.6 provisional** |
| lspSearch | 9 | 8 | 8 | 9 | 8 | 6 | 8 | **8.1 provisional** |
| graphBuild | 9 | 8 | 8 | 9 | 8 | 8 | 9 | **8.5** |
| graphQuery | 9 | 8 | 8 | 9 | 8 | 7 | 9 | **8.3** |
| ghSearchRepo | 9 | 9 | 8 | 9 | 8 | 5 | — | **8.1 provisional** |
| ghSearchCode | 9 | 9 | 8 | 10 | 9 | 6 | — | **8.5 provisional** |
| ghStructure | 10 | 9 | 8 | 10 | 9 | 6 | — | **8.8 provisional** |
| ghGetFileContent | 10 | 9 | 9 | 10 | 9 | 7 | 9 | **9.1** |
| ghSearchHistory | 9 | 9 | 8 | 9 | 9 | 5 | — | **8.2 provisional** |
| ghGetHistoryItem | 10 | 9 | 8 | 10 | 8 | 6 | — | **8.7 provisional** |
| ghCloneRepo | 10 | 9 | 8 | 10 | 9 | 7 | 10 | **9.0** |
| artifactSearch | 9 | 8 | 8 | 9 | 9 | 6 | 7 | **8.2** |
| securityCheck | 9 | 8 | 8 | 9 | 9 | 8 | — | **8.5 provisional** |

| Tool | Strength | Cost, limitation or gotcha |
|---|---|---|
| localSearch | Exact lexical matches, counts and freshness. | Node startup and richer output cost more than narrowly projected rg. |
| localFetch | Complete ranges, declarations and long Unicode values. | Tiny windows multiply requests; use ranges sized for the question. |
| structureSearch | Correct file membership and requested metrics. | Headers and optional leads exceed a narrow file-list projection. |
| astSearch | Structural matches and paged symbols agree with source. | Recovery lead was dropped; repaired. Syntax alone does not prove resolved identity. |
| clasify | Typed answers, source reads and zero-billing replay observed. | Large unread-corpus cost/recall remains unverified; configured provider required. |
| lspSearch | All 12 JS/TS operations exercised; resolved references agree. | Direct LSP is faster/smaller for narrow locations; Rust project-positive unverified. |
| graphBuild | Reproducible scoped graph, drift and rebuild behavior. | Requires setup; file-path error could give a runnable directory example. |
| graphQuery | All 21 operations exercised; witnesses and multi-page results preserved. | Rich answers are larger than Graphify projections; edges need semantic confirmation. |
| ghSearchRepo | Owner membership and selected facts match GitHub. | Mutable-metadata invalidation unverified; upstream sort order is not always monotonic. |
| ghSearchCode | File and distinct source-line evidence reconstructed. | Nested response/tool pagination requires care; lean gh can be smaller. |
| ghStructure | 46-file tree reconstructed with exact paths and sizes. | Known immutable refs benefit from explicit gh API caching too. |
| ghGetFileContent | Exact full source and oversized minified lines recoverable. | Lean raw readers are smaller when provenance is already known. |
| ghSearchHistory | Selected history and IDs agree with GitHub. | Search/provider ceilings and live ordering need explicit scope. |
| ghGetHistoryItem | Bodies, comments, reviews and patches remain reachable. | Independent continuation branches must all be completed. |
| ghCloneRepo | Sparse checkout bytes and HEAD match Git; ready local workflow. | Minimum-budget oversized marker is repaired; quiet Git emits fewer characters. |
| artifactSearch | Eight ecosystem resolutions and source/ref checks pass. | More expensive than registry-only projection; search coverage is npm-only. |
| securityCheck | Multiple provider records and differences are explicit. | JSON/MCP records pass independent source checks; readable numbers round and mutable freshness is unverified. |

## Scope and evidence

The original host release pin is `5950e9eb69eb31c890fa29c4c8d794595b4a0eab46f27aa44b9cf91cc74b52a0`. It supplied the measured comparisons below. The AST-only intermediate pin is `c614cca05c208e6ab446da2a7314a6016a57a91a5babcd925bbe9949c16139fb`. Final repaired-build evidence is separate; no original timing is relabeled as a final-build result. The final host release built in 180.06 seconds with stable source fingerprints and a matching embedded contract; [build provenance](../../../campaigns/20.2.0-2026-11-14-tool-ratings/verified4-build-provenance.json).

All retained receipts live in [this campaign](../../../campaigns/20.2.0-2026-11-14-tool-ratings/). The archive migration brought 14 prior benchmark-only locations into the package; 56 relocated symlink targets were checked with no mismatch. Raw records were not rewritten. See [archive locations](../../../ARCHIVES.md).

| Lane | Selected evidence and limits | Receipts |
|---|---|---|
| Local | 26 observed questions and 8 partial subchecks among 34. Search: 60 occurrences/30 lines over 8 pages. Fetch: 613-line declaration over 10 pages, 90-line file over 13, 27,009-byte Unicode line over 6. Structure: 11 files over 6 pages. AST: 8 symbols over 4 pages. Source edits refresh all four tools. | [Local report](../../../campaigns/20.2.0-2026-11-14-tool-ratings/local/REPORT.md), [metrics](../../../campaigns/20.2.0-2026-11-14-tool-ratings/local/metrics.json) |
| Classification | Correct port/theme/choice, three paged sections and pinned source reads; edits change the answer, replay bills zero. 28 original evaluation calls. Efficiency on a representative unread corpus remains unverified. | Local metrics and original raw receipts |
| Semantic | All 12 JS/TS LSP operations and all 21 graph query operations exercised. 8 exact references; mixed graph batch over 46 pages; 6 Git target commits/10 cochange rows. Rust detached-file recovery observed, positive Rust project semantics unverified. | [Semantic report](../../../campaigns/20.2.0-2026-11-14-tool-ratings/semantic/REPORT.md), [metrics](../../../campaigns/20.2.0-2026-11-14-tool-ratings/semantic/metrics.json) |
| GitHub | All 36 question outcomes recorded. 46 tree files/23 pages; 722 source lines/10 pages; 58 distinct path-line hits/13 calls; 32 repos/16 pages; 87,444-byte minified line/24 pages; 65 comment IDs, 13 PR files and 45 reviews. Three fresh sparse clones have exact bytes and HEAD. Mutable live metadata invalidation is not established. | [GitHub report](../../../campaigns/20.2.0-2026-11-14-tool-ratings/github/REPORT.md), [metrics](../../../campaigns/20.2.0-2026-11-14-tool-ratings/github/metrics.json) |
| Providers | Eight registry versions, three selected npm search results, source/tag qualifications and full selected dependencies. Security package IDs match OSV, but original advisory identity loss invalidated its efficiency comparison. A full native response alone is not a source oracle. | [Provider report](../../../campaigns/20.2.0-2026-11-14-tool-ratings/providers/REPORT.md), [proof](../../../campaigns/20.2.0-2026-11-14-tool-ratings/providers/correctness-proof.json) |

## Final rebuilt-pin verification

| Lane | Final selected evidence | Receipts |
|---|---|---|
| Eight local tools | 16 positive and 16 invalid executions; 24 complete chains / 403 page calls. All four declaration-order probes retain nine declarations and nine dependent hits in order. Complete 613-line Unicode declaration, 27,009-byte line, 11 file members, AST symbols/lead, resolved JS references and graph source witnesses. Classifier four provider calls (1,760 input / 160 output tokens); all source cursors checked. | [Report](../../../campaigns/20.2.0-2026-11-14-tool-ratings/final-local3/REPORT.md), [continuations](../../../campaigns/20.2.0-2026-11-14-tool-ratings/final-local3/continuation-edge-proof.json) |
| Seven GitHub tools | 290 successful page requests / 276 unchanged continuations. All ten selected code blobs independently fetched: 58 hits; 32 owner repos; 46 tree members; 722 source lines; 87,444 minified bytes; 72 complete selected history entries; PR body/13 files/45 reviews; issue body/65 complete comments. Atomic clone marker, HEAD and all 17 sparse-checkout files match Git objects. | [Report](../../../campaigns/20.2.0-2026-11-14-tool-ratings/github/final-pin-verified3/REPORT.md), [results](../../../campaigns/20.2.0-2026-11-14-tool-ratings/github/final-pin-verified3/RESULTS.json) |
| Two provider tools | 22 boundary/schema/paging checks and 16 parity assertions, including selected modern/legacy MCP calls. Security chains 17/23 pages and artifact details 3 pages on each surface (86 page calls). Fresh PyPI selected dependencies, Python requirement, license/version/summary match. Independent OSV/GitHub oracle passes 108/108 comparisons, with complete per-provider reference membership/order/types and affected metadata. | [Boundary proof](../../../campaigns/20.2.0-2026-11-14-tool-ratings/providers/final3/boundary-summary.json), [artifact source proof](../../../campaigns/20.2.0-2026-11-14-tool-ratings/providers/final3/artifact-source-proof.json), [independent security report](../../../campaigns/20.2.0-2026-11-14-tool-ratings/final-independent3/REPORT.md) |

The selected final chains total **779 page calls**, plus schema, boundary, oracle and related-call checks. This is not a claim that all 120 question subcases, every language or every API operation were rerun. Setup/comparator mistakes are retained and identified separately from product defects. GitHub folder `final-pin-verified3` contains the final **verified4** binary hash in its receipts; the folder name is not the executable identity.

The security defect cap is lifted only for the source-proven repaired pin; its 8.5 is a new weighted score with mutable freshness unknown. AST correctness is conservatively 9 because advanced competitor cases remain untested; repaired output/workflow raise their dimensions, leaving its overall 8.4. GitHub mutable-freshness gaps make five tools provisional; immutable file-ref separation and clone key/dirty-state invariants are credited only within their tested scope.

## Speed, output and caching

Three paired repetitions per selected comparison; descriptive samples on a shared host, not statistical generalizations. Natural output counts include stdout and stderr. JSON characters are not tokens. Setup, warm process state, explicit cache state and unchanged output are distinct. Every selected continuation must complete before a smaller or faster answer is eligible.

| Question | Measured result | Interpretation |
|---|---|---|
| Five narrow local tasks | Expert outputs smaller in 5/5; geometric mean Node/expert output 1.94×, MCP JSON/expert 2.43× | Rich provenance and leads carry a cost. Choose the requested representation. |
| Same localSearch text, direct native versus Node launcher | 11.03 ms versus 52.07 ms median; identical 1,196-character output | Substantial launcher/startup cost. This does not measure general search throughput. |
| Long declaration, complete chain | Node median 530 ms over ten 3,500-character pages | Page count is part of the workload; this is not default-window latency. |
| Graphify comparison | Fixture callers: Octocode 58.06 ms/1,022 chars, Graphify 134.93 ms/116 chars; real callers: 62.82 ms/4,115 versus 136.28 ms/674 | Octocode faster on these selected tasks, Graphify narrower. The same caller sets were checked; per-site evidence is richer in Octocode (19 Python sites across 16 callers versus 16 Graphify single-location edges). |
| Direct LSP comparison | Warm references: MCP 5.84 ms/2,537 chars; direct 1.65 ms/1,576 | Direct faster in all nine pairs. Octocode adds source, roles and diagnostics. |
| Six GitHub read comparisons | Lean gh smaller in 6/6; ordinary/lean/eligible cached tiers retained, 69 arm rows | Native usually faster than calls without an explicit gh cache flag; explicit immutable-ref gh cache can be comparable or faster. |
| Fresh sparse clone | Native median 1.632 s, quiet Git 2.045 s; same 17 files and HEAD | Native one setup call versus two; quiet Git emits zero characters, native receipt 764. |
| npm mapping | Node 615.78 ms, persistent MCP 296.48 ms, lean registry 66.49 ms, explicit local snapshot 4.09 ms | Selected version/repository/registry ref equal; native also checks and warns about tag disagreement. Snapshot fill/age are separate. |
| Security advisory GHSA-35jh-r3h4-6jhm, final pin | Node 261.13 ms/7,236 chars; MCP 214.13/7,592; lean serial 737.87/6,456; lean parallel 453.97/6,456; ordinary 767.46/11,185 | 15 observations are exact-source eligible; three readable Node observations are descriptive because EPSS 0.20158 rounds to 0.2016. MCP is faster on these warmed-client observations; lean output is smaller. Explicit snapshot 4.96 ms/6,431 chars, with 2,392.03 ms fill and TTL300 age recorded separately. |
| AST free calls, final pin | Node 56.49 ms/1,114 bytes (including stderr); MCP 2.90/1,032 content bytes; ast-grep 8.11/189 bytes | Three paired repetitions preserve the same calls and the restored native lead. Different paths/setup prevent a controlled old/new speed claim. |

Original timing sources: [local](../../../campaigns/20.2.0-2026-11-14-tool-ratings/local/metrics.json), [native control](../../../campaigns/20.2.0-2026-11-14-tool-ratings/providers/native-node-control.json), [semantic](../../../campaigns/20.2.0-2026-11-14-tool-ratings/semantic/metrics.json), [GitHub](../../../campaigns/20.2.0-2026-11-14-tool-ratings/github/timing-summary.json), [providers](../../../campaigns/20.2.0-2026-11-14-tool-ratings/providers/timing-summary.json). Readable numeric qualification: [offline field proof](../../../campaigns/20.2.0-2026-11-14-tool-ratings/final-independent3/natural-cli-selected-proof.json) and [eligibility addendum](../../../campaigns/20.2.0-2026-11-14-tool-ratings/providers/final3/timing/readable-eligibility-addendum.json). The readable formatter intentionally rounds fractional values; exact JSON/MCP comparisons remain eligible. Final timing receipts: [AST accounting](../../../campaigns/20.2.0-2026-11-14-tool-ratings/final-local3/ast-timing/accounting.json), [security metrics](../../../campaigns/20.2.0-2026-11-14-tool-ratings/providers/final3/timing/metrics.json), [security projection and cache plan](../../../campaigns/20.2.0-2026-11-14-tool-ratings/providers/final3/timing/plan.json). Provider origin-cache coldness is unknown unless a receipt establishes it. Fresh local clone directories do not establish a cold network origin.

## Repairs and verification

| Finding | Root cause and repair | Verification |
|---|---|---|
| AST-RELATED-01 | Shared workflow filtering dropped an existing `astSearch` call explicitly named by a diagnostic. Generic tool-name reference retention preserves that optional lead and the preferred read. | Red regression then 12 focused tests; intermediate and final fresh CLI/MCP emitted and executed the promised call; final paired timing includes its cost. |
| SECURITY-IDENTITY-01 | Advisory selection lost ecosystem/purl before paging; range dedup could merge distinct identities. Canonical core fields and native selection retain source identity. The subsequent full-record repair removes cross-provider deduplication entirely. | Core red then 20 tests green; native identity regressions included in canonical suite; final independent source oracle and CLI/MCP page reconstruction pass. |
| SECURITY-FIELD-DELTA-01 | A stronger provider oracle found that URL-only pruning loses reference membership/types while the then-contract claimed an absent field inherits the first record. Partial affected arrays, cross-record fix suppression and opaque metadata stripping have the same source-preservation risk. | Original and held-out provider reference arrays fail the stronger oracle even though URL-union coverage passes. Source repair implemented: independent selected source records, full selected metadata and same-record fix redundancy only. Thirteen new regressions failed first; all 291 focused security tests then passed with stable source hashes and independent review. Final 108/108 independent source checks and all rebuilt page walks pass; cap removed for this pin; [failed oracle](../../../campaigns/20.2.0-2026-11-14-tool-ratings/providers/SECURITY-FIELD-DELTA-01/reviewer-oracle.md). |
| CLONE-OVERSIZED-01 | Clone correctly bypasses replay pagination but bypassed oversized disclosure too. Shared unpaged handling preserves the receipt and marks indivisible oversized evidence. | Original CLI/MCP red receipt retained; 21 focused tests and verified2 CLI/MCP passed. Final rebuilt CLI/MCP marker and exact clone source checks pass. |
| DECLARATION-ORDER-01 | A later shared-tree collection splitter used schema traversal rank as document order. Tiny real localSearch pages then delivered hits before the declarations indexed by `enclosing`. All values eventually arrived. | Verified2 CLI/MCP fails all four 400/600-window probes; original5950 same-input unsplit response does not fail. Restored strict regression and generic document-order repair passed focused tests and full final4 CI; all four final rebuilt CLI/MCP probes pass with exact source and declaration-before-hit order; see [probe evidence](../../../campaigns/20.2.0-2026-11-14-tool-ratings/final-local/pagination-probe/REPORT.md). |
| KEY-ORDER-01 | Concurrent list-key ordering prepended unanchored new keys and broke existing order assertions. The converged repair appends unanchored additions while retaining insertion after an observed predecessor. | Eight focused tests passed; existing assertions were preserved and a combined regression was added. Full CI then passed without drift. Product repair was concurrent work, independently reviewed; [source witnesses](../../../campaigns/20.2.0-2026-11-14-tool-ratings/semantic/KEY-ORDER-01/receipt.json). |

The final6 canonical Rust suite passed **6,985 native/runtime/CLI/provider tests and 1,664 engine tests: 8,649 passed, 16 skipped, zero failures**, with no source fingerprint drift. Rust lint passed. The host release build, embedded-contract check and all named final runtime checks passed. Earlier passes and failures remain retained with their source fingerprints; the first passing round had ten concurrent source changes and is not used as an exact-tree certificate. See [suite receipt](../../../campaigns/20.2.0-2026-11-14-tool-ratings/final6-suite-result.json) and [lint receipt](../../../campaigns/20.2.0-2026-11-14-tool-ratings/lint-final6-result.json).

Two test corrections are separate from product repairs. `PR-ROW-PAGINATION-01` permits a valid
file-summary fragment before patch data while retaining exact inventory, patch, offset and uniqueness
assertions. `ALIGNMENT-OWNERSHIP-01` limits repeated-field candidates to fields owned by the published
schema; opaque provider metadata and dynamic extensions retain their source-local evidence. Six new
checker regressions passed on the first implementation run. Exactly five obsolete graph-extension
exceptions were removed; the declared `rows[].sites` exception remains. The focused rerun passed 7/7 with stable source hashes, and the subsequent full suite passed. See the [PR test review](../../../campaigns/20.2.0-2026-11-14-tool-ratings/github/PR-ROW-PAGINATION-01.md)
and [checker review](../../../campaigns/20.2.0-2026-11-14-tool-ratings/github/ALIGNMENT-OWNERSHIP-01/reviewer-approval.md).

## API, schemas and follow-up work

All 17 canonical tools were inventoried; the original 60,805-character input inventory combined 16 isolated-profile schemas with a standalone classifier schema; it was not one configured-session total. The final authenticated 17-tool session advertises **64,208** compact JSON input-schema characters and **12,171** description characters. Classification adds fields, conditional guidance and definition sharing to seven other schemas (3,401 characters); the isolated 16-tool profile totals 52,656. These are codepoints, not tokens or a size-regression comparison; [profile proof](../../../campaigns/20.2.0-2026-11-14-tool-ratings/final-schema-profile-difference.json). The largest schemas are `clasify`, `graphQuery` and `astSearch`. The connected server may expose fewer tools because opt-in configuration differs; this round launches the local pin explicitly. MCP outputSchema is optional and its absence alone is not scored as a defect. The server advertises per-tool output field/collection references through resources/list and resources/read; effective CLI/MCP input schema, description and output-reference equality passed for all 17 tools in their configured profiles.

The security contract change originates in sibling core, passes through the one config generator, and is embedded in native. No hand-written wire copy or runtime contract override was added. Rust fixes reuse existing ownership and visitor helpers, add no unsafe code or dependency, and protect evidence invariants with failing-first regressions. This scoped API review does not claim a fresh audit of every Rust API.

[Optimizations and checked best practices](OPTIMIZATIONS.md) cover persistent MCP, native versus Node startup, narrow rg/ast-grep/gh projections, explicit cache tiers, page workflows and evidence-preserving output improvements. The earlier [advanced capability round](../20.2.0-2026-11-14-advanced/SCORECARD.md) remains separately pinned.

A concurrent core edit renamed structureSearch’s sort value from `lineCount` to `totalLines`.
Regeneration exposed six stale native enum references. Root aligned those callers and sixteen query
values across fourteen source/test files without changing sorting logic or assertions. The
[mechanical-change receipt](../../../campaigns/20.2.0-2026-11-14-tool-ratings/structure-contract-sync.json)
and retained failed compile distinguish this final integration from original-pin measurements.

## Final review and cleanup

Astra independently approved the score arithmetic, evidence links, version separation and bounded claims, including the readable-number qualification: [final review](../../../campaigns/20.2.0-2026-11-14-tool-ratings/final-report-review.md). No production source changed after the verified build; the staged host binary still matches the tested hash. Owned runtime processes, isolated homes and five test pins were removed. Normal credentials/caches, foreign processes and the dirty semantic fixture were preserved. Rust pruning removed 368.7 MiB; disk check reports 486 GiB available. No commit, stash or publication was performed. [Cleanup](../../../campaigns/20.2.0-2026-11-14-tool-ratings/final-cleanup.json), [process retirement](../../../campaigns/20.2.0-2026-11-14-tool-ratings/root-process-retirement-final.json), [pruning and disk](../../../campaigns/20.2.0-2026-11-14-tool-ratings/prune-rust-final.json).

Skill checks passed (78 self-test cases and 48 Node tests); final documentation and whitespace gates passed. The original three large-scope gaps remain: representative unread-corpus classification efficiency, positive Rust-project LSP semantics, and mutable GitHub/advisory cache invalidation.
