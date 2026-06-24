# Octocode Search Flow Coverage

Created: 2026-06-24T11:23:13.435Z
Output: /Users/guybary/Documents/octocode-mcp/packages/octocode-benchmark/output/search-flow-coverage-2026-06-24T11-21-34-295Z
Rows: 24 (23 pass, 1 partial, 0 fail)
Score: 47/48
Search-only verified: true
Materialized path: /Users/guybary/.octocode/tmp/clone/pmndrs/zustand/main__sp_25a663

## Coverage
- Local: structure view, path search, content search, match-string fetch in none/standard/symbols modes, and AST structural search.
- GitHub provider: repository search, structure view, provider path search via `--match path`, content/code search, and match-string fetch in none/standard/symbols modes.
- GitHub to local: `target:"materialize"` via `search --query`, then local structure/path/content/fetch/AST over the returned `localPath`, plus direct `--repo ... --materialize required`.

| ID | Lane | Status | ms | Results | Diagnostics | Evidence | Notes |
|---|---|---:|---:|---:|---:|---|---|
| LOCAL-STRUCTURE | local | pass | 287 | 14 | 0 | proof | passed |
| LOCAL-PATH-SEARCH | local | pass | 239 | 1 | 0 | proof | passed |
| LOCAL-CONTENT-SEARCH | local | pass | 221 | 3 | 0 | proof | passed |
| LOCAL-FETCH-NONE | local | pass | 590 | 1 | 1 | proof partial | passed with 1 diagnostic(s) |
| LOCAL-FETCH-STANDARD | local | pass | 587 | 1 | 1 | proof partial | passed with 1 diagnostic(s) |
| LOCAL-FETCH-SYMBOLS | local | pass | 610 | 1 | 0 | proof | passed |
| LOCAL-AST-STRUCTURAL | local | partial | 242 | 10 | 1 | partial partial | partial with continuation |
| GH-REPO-SEARCH | github-provider | pass | 1281 | 1 | 0 | proof | passed |
| GH-STRUCTURE | github-provider | pass | 1340 | 17 | 0 | proof | passed |
| GH-PATH-SEARCH-PROVIDER | github-provider | pass | 1004 | 12 | 1 | proof | passed with 1 diagnostic(s) |
| GH-PATH-SEARCH-FILES-DIAGNOSTIC | github-provider | pass | 225 | 0 | 2 | unsupported partial | expected diagnostic route honesty |
| GH-CONTENT-SEARCH | github-provider | pass | 1127 | 5 | 1 | partial partial | passed with 1 diagnostic(s) |
| GH-FETCH-NONE | github-provider | pass | 1590 | 1 | 0 | proof | passed |
| GH-FETCH-STANDARD | github-provider | pass | 1666 | 1 | 0 | proof | passed |
| GH-FETCH-SYMBOLS | github-provider | pass | 1649 | 1 | 0 | proof | passed |
| MAT-MATERIALIZE | github-materialized-local | pass | 1096 | 1 | 1 | proof | passed with 1 diagnostic(s) |
| MAT-STRUCTURE | github-materialized-local | pass | 210 | 10 | 0 | proof | passed after src-path rerun |
| MAT-PATH-SEARCH | github-materialized-local | pass | 212 | 1 | 0 | proof | passed |
| MAT-CONTENT-SEARCH | github-materialized-local | pass | 225 | 5 | 0 | partial partial | passed |
| MAT-FETCH-NONE | github-materialized-local | pass | 436 | 1 | 1 | proof partial | passed after src-path rerun with 1 diagnostic(s) |
| MAT-FETCH-STANDARD | github-materialized-local | pass | 426 | 1 | 1 | proof partial | passed after src-path rerun with 1 diagnostic(s) |
| MAT-FETCH-SYMBOLS | github-materialized-local | pass | 447 | 1 | 0 | proof | passed after src-path rerun |
| MAT-AST-STRUCTURAL | github-materialized-local | pass | 215 | 1 | 0 | proof | passed after src-path rerun |
| GH-REMOTE-AS-LOCAL-DIRECT | github-materialized-local | pass | 1124 | 5 | 1 | partial partial | passed with 1 diagnostic(s) |

## Findings
- GitHub `--search path` / target files with basename predicates is not provider-native today; it reports `requiresMaterialization`. The search-only route is either provider code search with `--match path` for candidate paths, or `target:"materialize"` followed by local `--search path` for exact file rows.
- GitHub code search returns path-level candidates, not line/AST proof. The flow correctly follows with match-string content fetch for exact lines, or materializes and continues locally for line-level/AST proof.
- Minify modes are covered on both local files and GitHub files: `none` for exact proof, `standard` for compact content, `symbols` for skeleton/orientation.
- Materialization returns the sparse clone repo root; follow into `src/` for subtree structure and `src/vanilla.ts` for content/AST proof.

## Raw Evidence
- Raw stdout/stderr for every row is under `raw/`.
- `commands.ndjson` is the command ledger; every command starts with `node packages/octocode/out/octocode.js search`.
