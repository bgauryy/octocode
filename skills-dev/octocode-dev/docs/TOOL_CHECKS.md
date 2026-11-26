Load when writing or reviewing the tests of one named tool, or its minification and window cases.

# Per-tool checks

What each tool's tests cover, beyond the [acceptance rules](TOOL_QUALITY.md#acceptance) every tool meets.

## Per tool

| Tool | Check |
|---|---|
| `localSearch` | Text and regex queries independently; match continuations, exclusions, zero-result diagnostics. |
| `localFetch` | Path-only, full, range, match and each view; matched anchors, transformed windows, fallback mode, redaction, source lines. |
| `clasify` | Noul, Choice, Score over values and unread resources; body-free answers, coverage, focus scopes, provider failures, cache reuse on exact reads. |
| `structureSearch` | `view:"paths"` and `"details"` with name and metadata filters; listing and tree pagination, snapshots, depth bounds, exclusions. |
| `astSearch` | `match`, `syntaxTree`, `symbols`; structural `rule` composition, parser selection, captures, nodes, parser and scan limits. |
| `lspSearch` | Document, workspace, anchored, hierarchy operations; no server vs unsupported capability vs failed anchor vs valid empty; provenance; snapshots. |
| `graphBuild` | Scope, `refresh`, storage, beta availability, one query per batch; freshness, durable extraction resume, source invalidation, bounded spill, failed-build preservation, corrupt checkpoint rejection. |
| `graphQuery` | Every operation, selector, witness and resolution option; pinned continuation union per collection; integrity errors, stale snapshots, grammar coverage, unresolved sites. |
| `ghSearchRepo`, `ghSearchCode`, `ghStructure` | Code search uses the default-branch index, and an explicit `ref` requires `repo` and re-reads its hits; immutable tree identity, metadata pages, indexing uncertainty, provider-limit diagnostics. |
| `ghGetFileContent` | `content`, `compact`, `symbols`, full and windowed; local/remote parity, pinned refs, repeated-outline prevention, redaction, directory-input rejection. |
| `ghSearchHistory` | PR, issue, commit discovery with operation scope; pages, filters, field selection, provider-incomplete results. |
| `ghGetHistoryItem` | PR/issue, commit, compare; files, comments, reviews, commits, bodies and patches each traversed alone; omitted vs empty patch; immutable refs. |
| `ghCloneRepo` | Default ref, full SHA, sparse paths, full history, availability gates; ref resolved to a commit every call, a clean checkout reused without a fetch, local edits refused and preserved, concurrent mutation, rollback, crash recovery. |
| `artifactSearch` | Exact, scoped, keyword (mixed or empty selectors rejected); keyword continuations, repo subdirectory, registry failures, authenticated registries. |
| `securityCheck` | Every `operation`; `url` forms (purl, registry page, repository, commit, advisory); `release` on distributions only. The verdict equals OSV's answer; repository facts and advisories equal GitHub's REST record; cvss `vector` never a number, `score` only as GitHub publishes it; a refused GitHub read is disclosed (`sourceFailed`), never clean. |

Minification and windows: file fetch (local and GitHub) tests `content`, `compact`, `symbols`, selectors and line or byte windows against exact source, outline, fallback metadata and the continuation union. GitHub code and history search take `fields` only, no `minify`. PR, issue, commit and compare detail stay GitHub's verbatim text; commit and compare also test `sections:["patches"]`, `include` path selection and absent vs omitted vs empty patches. Build the native minification matrix from runtime configuration, filename overrides included ([supported languages](../../../packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md)); fixtures cover delimiter comments, comment-like strings, TS types and imports, JSX/TSX, data, markup, indentation-sensitive, empty, malformed and near-limit files, checked for validity and preservation, not byte counts.
