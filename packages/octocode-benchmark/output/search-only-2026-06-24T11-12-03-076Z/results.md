# Octocode Search-Only Benchmark

Created: 2026-06-24T11:13:48.898Z
Output: /Users/guybary/Documents/octocode-mcp/packages/octocode-benchmark/output/search-only-2026-06-24T11-12-03-076Z
Rows: 23 (20 pass, 3 partial, 0 fail)
Score: 43/46
Search-only verified: true

| ID | Area | Status | ms | Results | Diagnostics | Evidence | Notes |
|---|---|---:|---:|---:|---:|---|---|
| SEARCH-SCHEME-01 | schema | pass | 118 | - | 0 | - | text checks passed |
| LCL-CONTENT-01 | local-content | pass | 607 | 1 | 1 | proof partial | passed with 1 diagnostic(s) |
| LCL-CODE-01 | local-code | pass | 210 | 3 | 0 | proof | passed |
| LCL-CODE-ONLYMATCH-01 | local-code | pass | 224 | 10 | 1 | partial partial | passed with 1 diagnostic(s) |
| LCL-AST-01 | local-ast | partial | 215 | 10 | 1 | partial partial | partial with continuation |
| LCL-STRUCTURE-01 | local-structure | pass | 210 | 18 | 0 | proof | passed |
| LCL-FILES-01 | local-files | pass | 2945 | 20 | 0 | proof | passed after path-discovery rerun |
| LCL-SEMANTICS-SYMBOLS-01 | local-semantics | partial | 692 | 1 | 1 | partial partial | partial with continuation |
| LCL-SEMANTICS-REFS-01 | local-semantics | pass | 895 | 1 | 0 | proof | passed |
| LCL-DIFF-01 | local-diff | pass | 482 | 1 | 1 | proof | passed with 1 diagnostic(s) |
| LCL-ARTIFACT-LIST-01 | local-artifacts | pass | 476 | 1 | 0 | proof | passed |
| LCL-ARTIFACT-EXTRACT-01 | local-artifacts | pass | 453 | 1 | 0 | proof | passed |
| LCL-ARTIFACT-STRINGS-01 | local-artifacts | pass | 433 | 1 | 0 | proof | passed |
| LCL-RESEARCH-01 | local-research | partial | 220 | 1 | 0 | candidate partial | partial with continuation |
| EXT-PACKAGES-01 | external-packages | pass | 2154 | 1 | 0 | proof | passed |
| EXT-REPOSITORIES-01 | external-repositories | pass | 1404 | 5 | 0 | partial partial | passed |
| EXT-CODE-01 | external-code | pass | 1309 | 5 | 1 | partial partial | passed with 1 diagnostic(s) |
| EXT-STRUCTURE-01 | external-structure | pass | 1336 | 26 | 0 | proof | passed |
| EXT-PULLREQUESTS-01 | external-prs | pass | 1141 | 1 | 0 | partial partial | passed |
| EXT-COMMITS-01 | external-commits | pass | 1109 | 1 | 0 | partial partial | passed |
| EXT-DIFF-01 | external-diff | pass | 467 | 1 | 1 | proof | passed with 1 diagnostic(s) |
| EXT-REMOTE-AS-LOCAL-01 | external-materialized | pass | 1219 | 5 | 1 | proof | passed with 1 diagnostic(s) |
| EXT-MATERIALIZE-01 | external-materialized | pass | 4040 | 1 | 0 | proof | passed |

## Issues and Gaps
- LCL-CONTENT-01: passed with 1 diagnostic(s) (partialResult: Found 3 occurrences of "runOqlSearch" on lines 16, 35, 240 — all shown as 3 slices, ±4 lines of context each; these lines are lineHint anchors for lspGetSemantics.)
- LCL-CODE-ONLYMATCH-01: passed with 1 diagnostic(s) (matchTruncated: 1 file(s) have more matches (showed 10 of 178) — page with controls.search.matchPage, or raise controls.search.maxMatchesPerFile.)
- LCL-AST-01: partial with continuation (matchTruncated: 1 file(s) have more matches (showed 10 of 191) — page with controls.search.matchPage, or raise controls.search.maxMatchesPerFile.)
- LCL-SEMANTICS-SYMBOLS-01: partial with continuation (partialResult: Semantic result is paginated; follow the continuation before treating it as complete proof.)
- LCL-DIFF-01: passed with 1 diagnostic(s) (zeroMatches: Files are identical.)
- LCL-RESEARCH-01: partial with continuation
- EXT-CODE-01: passed with 1 diagnostic(s) (providerSemanticsApproximate: GitHub code search returns path-level hits without line numbers; follow next.fetch for exact location/lines.)
- EXT-DIFF-01: passed with 1 diagnostic(s) (zeroMatches: Files are identical at both refs.)
- EXT-REMOTE-AS-LOCAL-01: passed with 1 diagnostic(s) (providerSemanticsApproximate: GitHub code search returns path-level hits without line numbers; follow next.fetch for exact location/lines.)

## Correction
- LCL-FILES-01 was rerun with `search package.json . --search path --name package.json --details --limit 20 --json --compact`; the first run used `--target files` in a way that created a boolean content+name predicate and returned zero matches.

## Good Flows
- `search <file> --op documentSymbols` produced the `searchCommand` line and the follow-up `--op references --line <line>` completed as semantic proof.
- `search --pattern ... --lang ts` returned AST-backed hits with `next.fetch` and match-page continuation, which is enough for an agent to prove instead of guessing.
- Archive list then extract stayed entirely in `search --target artifacts` and produced an exact extracted content proof.
