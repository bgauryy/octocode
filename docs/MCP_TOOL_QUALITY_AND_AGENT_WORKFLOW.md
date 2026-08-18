# Agent workflow and tool improvement notes

This page gives the recommended agent workflow for Octocode tools, the response
contracts that all tools share, and the open improvement notes per tool. It is
written for contributors and for agents tuning their own tool use.

To exercise the full tool surface locally, start the built MCP stdio server with
every optional surface turned on:

```bash
ENABLE_LOCAL=true \
ENABLE_CLONE=true \
ENABLE_RELEASES=1 \
ENABLE_DISCUSSIONS=1 \
ENABLE_TOOLS=ghListReleases,ghSearchDiscussions \
OCTOCODE_TRUST_PROJECT_LSP_CONFIG=1 \
node packages/octocode-mcp/dist/index.js
```

The per-tool notes below do not yet cover `localFindDeadCode` or `ghSearchDiscussions`.

## Standard agent workflow

Use this workflow by default. It keeps cost low, raises proof quality step by step, and avoids treating discovery as proof.

```text
1. Orient
   localViewStructure / ghViewRepoStructure / npmSearch / ghSearchRepos

2. Discover candidates
   localFindFiles / localSearchCode / ghSearchCode / ghSearchCommits / ghSearchPullRequests

3. Inspect exact evidence
   localGetFileContent / ghGetFileContent

4. Prove identity or impact
   lspGetSemantics for definitions, references, hover, callers/callees, diagnostics

5. Escalate remote to local when needed
   ghCloneRepo -> localViewStructure -> localSearchCode -> localGetFileContent -> lspGetSemantics

6. Synthesize with evidence status
   Mark every claim as: orientation, candidate, exact text, semantic proof, or historical proof.
```

### Default local-code workflow

```text
localViewStructure(path)
  -> localFindFiles(path, names/extensions/metadata)
  -> localSearchCode(path, text/regex/AST)
  -> localGetFileContent(path, line range or matchString)
  -> lspGetSemantics(uri, symbolName, lineHint)
```

Rules:

- Use `localViewStructure` first for unknown directories.
- Use `localSearchCode(mode:"discovery")` when snippets are not needed.
- Use `localSearchCode(mode:"structural")` for code shape.
- If structural `pattern` returns zero, try a YAML `rule`.
- Use `localGetFileContent(minify:"symbols")` before reading large files.
- Use LSP only after anchoring a real symbol line from search/symbols.

### Default GitHub workflow

```text
ghSearchRepos / npmSearch
  -> ghViewRepoStructure
  -> ghSearchCode(match:"path" for filenames, match:"file" for snippets)
  -> ghGetFileContent(range/matchString/symbols)
  -> ghCloneRepo when cross-file or LSP proof is needed
  -> local tools on cloned localPath
```

Rules:

- Do not treat empty GitHub search as proof of absence; GitHub is indexed/default-branch-limited.
- Use `ghViewRepoStructure` to verify path case before `ghGetFileContent`.
- Use `ghCloneRepo` for multi-file reasoning, large investigations, or semantic proof.

### Default history workflow

```text
ghSearchCommits(owner, repo, path?)
  -> ghSearchPullRequests(owner, repo, prNumber, content)
  -> ghGetFileContent(branch/SHA, path)
  -> ghCloneRepo(branch/SHA) for deeper local inspection
```

Rules:

- Start with concise commit/PR listing.
- Fetch selected changed files or selected patch ranges before full patches.
- Treat PR comments as context, not code proof.

### Default package workflow

```text
npmSearch(packageName)
  -> source repository URL
  -> ghViewRepoStructure / ghSearchCode
  -> ghCloneRepo for local proof
```

Rules:

- Prefer exact package lookup over keyword lookup when you know the name.
- Use package health fields for triage, not correctness proof.

---

## Suite-wide improvement contracts

The following contracts are the highest-impact improvements across the whole tool suite.

### 1. Standardize `next` hints across all tools

Every result must suggest the next best tool. A `next` entry must be directly callable, explain why it helps, and state its confidence.

Recommended shape:

```ts
next: {
  [key: string]: {
    tool: string;
    query: Record<string, unknown>;
    why: string;
    confidence: 'exact' | 'high' | 'medium' | 'low';
  };
}
```

Expected chains:

| Result source | Required next hints |
|---|---|
| `localSearchCode` hit | `localGetFileContent`, `lspGetSemantics` when a symbol anchor is available. |
| `localFindFiles` file | `localGetFileContent`, `localSearchCode` scoped to the file/directory. |
| `localViewStructure` file | `localGetFileContent`. |
| `localViewStructure` directory | deeper `localViewStructure`, `localFindFiles`, `localSearchCode`. |
| `ghSearchCode` hit | `ghGetFileContent`, `ghViewRepoStructure`, `ghCloneRepo`. |
| `ghSearchRepos` repository | `ghViewRepoStructure`, `ghSearchCode`, `ghCloneRepo`. |
| `ghViewRepoStructure` file | `ghGetFileContent`. |
| `ghViewRepoStructure` directory | deeper `ghViewRepoStructure`, `ghCloneRepo`. |
| `ghGetFileContent` file | `ghCloneRepo` for multi-file/LSP work. |
| `ghCloneRepo` result | `localViewStructure`, `localSearchCode`, `localFindFiles`. |
| `npmSearch` GitHub repository | `ghViewRepoStructure`, `ghSearchCode`, `ghCloneRepo`. |
| `ghSearchPullRequests` changed file | `ghGetFileContent`, selected patch follow-up, `ghCloneRepo`. |
| `ghSearchCommits` commit | `ghGetFileContent` at SHA, selected diff follow-up, `ghCloneRepo`. |
| `lspGetSemantics` references | `localGetFileContent` around reference lines, callers/callees follow-up. |

Acceptance criteria:

- Each major row type has at least one `next` hint.
- Every `next.query` can be passed to the named tool without manual path conversion.
- Hints never claim proof; they explain the next evidence step.

### 2. Standardize diagnostics/warnings

Outputs use a mix of `warnings`, errors, diagnostics, text guidance, and `next`. Keep compatibility, but converge on a common `diagnostics` shape.

Recommended shape:

```ts
diagnostics: [
  {
    severity: 'info' | 'warning' | 'error';
    code: string;
    message: string;
    blocksAnswer?: boolean;
    queryPath?: string;
    repair?: {
      tool?: string;
      query?: Record<string, unknown>;
      message?: string;
    };
  }
]
```

Rules:

- `error`: tool could not answer the requested query.
- `warning`: answered, but evidence may be partial, capped, paginated, fallback-based, or lossy.
- `info`: useful repair/ergonomic guidance, such as AST pattern tips.
- Existing `warnings` can remain, but agents must also receive equivalent structured diagnostics.

Acceptance criteria:

- Zero-match structural search emits a structured diagnostic with a `repair` rule example.
- GitHub empty search emits an indexing/branch uncertainty diagnostic.
- Large-file rejection emits diagnostics with line-range/pagination repair examples.
- LSP unavailable/unsupported/anchor-failed states have distinct diagnostic codes.

### 3. Normalize path/URI display

Local and LSP flows must display paths consistently while preserving the exact machine fields.

Recommended shape:

```ts
{
  path: 'packages/octocode-tools-core/src/serverConfig.ts',
  absolutePath: '/home/username/repo/packages/octocode-tools-core/src/serverConfig.ts',
  uri: 'file:///home/username/repo/packages/octocode-tools-core/src/serverConfig.ts'
}
```

Rules:

- `path`: human-friendly repository-relative path when a workspace/root is known.
- `absolutePath`: direct local filesystem path for local tools.
- `uri`: file URI for LSP operations.
- Never pass `file://` URIs to filesystem-only functions.
- Never pass plain paths to LSP calls that require URIs unless the tool explicitly converts them.

Acceptance criteria:

- LSP reference results do not mix unexplained plain paths and `file://` URIs.
- Every local file row can feed `localGetFileContent` without conversion.
- Every semantic row can feed `lspGetSemantics` without guessing URI/line.

### 4. Standardize evidence levels and proof-upgrade guidance

Agents need to know whether a result is orientation, candidate evidence, exact evidence, semantic proof, or historical context. The following table maps each tool to its default level:

Recommended shape:

```ts
evidence: {
  level: 'orientation' | 'candidate' | 'exact-text' | 'semantic-proof' | 'historical-context' | 'package-metadata';
  complete: boolean;
  limitations?: string[];
  upgrade?: {
    tool: string;
    query: Record<string, unknown>;
    why: string;
  };
}
```

| Tool/result | Default evidence level | Upgrade path |
|---|---|---|
| `localViewStructure` | `orientation` | `localSearchCode` or `localGetFileContent`. |
| `localFindFiles` | `candidate` | `localGetFileContent` or `localSearchCode`. |
| `localSearchCode` text hit | `exact-text` for snippet presence, not symbol identity | `lspGetSemantics`. |
| `localSearchCode` structural hit | `candidate` to `exact-text` depending row | `localGetFileContent`, then `lspGetSemantics`. |
| `localGetFileContent` | `exact-text` | `lspGetSemantics` for identity/impact. |
| `lspGetSemantics` | `semantic-proof` | read reference/caller lines for quotes. |
| `ghSearchRepos` | `orientation` | `ghViewRepoStructure`/`ghSearchCode`. |
| `npmSearch` | `package-metadata` | GitHub tools or clone. |
| `ghSearchCode` | `candidate` or `exact-text` snippet | `ghGetFileContent`/`ghCloneRepo`. |
| `ghViewRepoStructure` | `orientation` | `ghGetFileContent`/`ghCloneRepo`. |
| `ghGetFileContent` | `exact-text` | clone for local proof. |
| `ghSearchPullRequests` | `historical-context` | fetch exact files/patches. |
| `ghSearchCommits` | `historical-context` | fetch exact files/diffs at SHA. |
| `ghCloneRepo` | `orientation` bridge | local tools. |

Acceptance criteria:

- Agents can distinguish "found a candidate" from "proved identity".
- Every non-proof result includes an upgrade suggestion.
- Search relevance order is never presented as proof.

---

## Per-tool documentation and improvements

### `localSearchCode`

Role: search local files by text, regex, or AST/structural rule.

Best for:

- Finding candidate code locations.
- Fast repository-wide text search.
- Structural code-shape discovery.
- Producing anchors for exact reads and LSP proof.

Use when:

- You know a symbol, phrase, import, error text, or code shape.
- You need candidate files/lines before reading content.

Avoid when:

- You only need directory shape: use `localViewStructure`.
- You only need file metadata/name filters: use `localFindFiles`.
- You already need semantic identity: use `lspGetSemantics` after anchoring.

Depth/alignment/quality:

- Depth: **9.5/10**
- Alignment: **9.6/10**
- Quality: **9.0/10**

Current strengths:

- Native ripgrep and structural engine.
- Detailed stats and pagination.
- Multiple modes: discovery, paginated, detailed, structural.
- Good bridge to `localGetFileContent` and `lspGetSemantics`.

Improvements:

1. Add structured diagnostics for structural zero matches.
2. Add `next.tryRule`, `next.tryTextSearch`, and `next.fetchContent` repairs.
3. Detect shell-expanded AST metavars in suspicious patterns and warn.
4. Add language-specific structural examples.
5. Emit evidence level: text hit vs structural candidate.

### `lspGetSemantics`

Role: semantic code intelligence: symbols, definitions, references, hover, diagnostics, call/type hierarchy.

Best for:

- Proving symbol identity.
- Finding true references/callers/callees.
- Getting type/hover information.
- Validating diagnostics.

Use when:

- You have a real file and line anchor.
- You need proof beyond text matching.

Avoid when:

- You have no anchor yet: first run `documentSymbols` or `localSearchCode`.
- Language server is unavailable and text proof is enough.

Depth/alignment/quality:

- Depth: **9.3/10**
- Alignment: **9.5/10**
- Quality: **8.8/10**

Current strengths:

- Native LSP path works through MCP.
- Supports high-value semantic operations.
- Strong proof-upgrade step after search.

Improvements:

1. Normalize path/URI display in all result rows.
2. Add distinct diagnostics for server unavailable, unsupported language, startup failed, and anchor failed.
3. Add `proofGrade: "lsp"` and server identity fields.
4. Add recovery `next.documentSymbols` when anchor resolution fails.
5. Add content-read next hints around reference/caller lines.

### `localGetFileContent`

Role: read exact local file content by range, match, full content, or minified/symbol view.

Best for:

- Quoting exact evidence.
- Reading around search/LSP anchors.
- Inspecting symbols without loading a whole large file.

Use when:

- You have a file path and need content.
- You need bounded exact evidence.

Avoid when:

- You only need file discovery: use `localFindFiles`/`localSearchCode`.
- You need semantic references: use `lspGetSemantics` after reading/anchoring.

Depth/alignment/quality:

- Depth: **9.0/10**
- Alignment: **9.3/10**
- Quality: **9.0/10**

Current strengths:

- Bounded reads and pagination.
- `symbols`, `standard`, and exact `none` views.
- Good exact-evidence tool.

Improvements:

1. Add `symbolCount` and outline metadata in `symbols` mode.
2. Add `next.documentSymbols` for source files.
3. Add `next.nextPage` and `next.readAroundLine` consistently.
4. Improve large-file rejection repairs.
5. Emit evidence level `exact-text`.

### `localFindFiles`

Role: local file/directory discovery by name, type, size, time, permissions, and depth.

Best for:

- Locating files by metadata.
- Repository orientation by filename/extension.
- Finding recently changed/stale/large files.

Depth/alignment/quality:

- Depth: **8.4/10**
- Alignment: **8.6/10**
- Quality: **8.7/10**

Improvements:

1. Add extension histogram summaries.
2. Add default exclude-dir diagnostic.
3. Add `next.localGetFileContent` for file rows.
4. Add `next.localSearchCode` scoped to matched directories.
5. Emit evidence level `candidate`.

### `localViewStructure`

Role: local directory tree orientation.

Best for:

- Understanding repository or directory shape.
- Cheap first step before deeper search/read.

Depth/alignment/quality:

- Depth: **8.3/10**
- Alignment: **8.7/10**
- Quality: **8.6/10**

Improvements:

1. Add extension/file-count summaries.
2. Add `next` hints for files and directories.
3. Make pagination/large-tree narrowing more prominent.
4. Clarify `pattern` is name/path filtering, not content search.
5. Emit evidence level `orientation`.

### `ghSearchCode`

Role: search GitHub code or paths.

Best for:

- Remote candidate discovery.
- Finding files/snippets before exact remote read or clone.

Depth/alignment/quality:

- Depth: **8.2/10**
- Alignment: **8.4/10**
- Quality: **8.0/10**

Improvements:

1. Add GitHub indexing uncertainty diagnostic on empty results.
2. Add `match:"path"` vs `match:"file"` repair guidance.
3. Add `next.ghGetFileContent`, `next.ghViewRepoStructure`, and `next.ghCloneRepo` for each result.
4. Surface rate-limit/incomplete-result metadata when GitHub provides it.
5. Emit evidence level `candidate` for search rows and `exact-text` only for returned snippets.

### `ghGetFileContent`

Role: read exact GitHub file content or materialize a directory subtree.

Best for:

- Remote exact evidence.
- Reading line ranges or match slices from GitHub.
- Verifying paths found by `ghSearchCode` or `ghViewRepoStructure`.

Depth/alignment/quality:

- Depth: **8.5/10**
- Alignment: **8.8/10**
- Quality: **8.5/10**

Improvements:

1. Add stronger 404 recovery diagnostics: verify branch, path case, and tree with `ghViewRepoStructure`.
2. Add `next.ghCloneRepo` when a file read suggests cross-file investigation.
3. Align content metadata with `localGetFileContent`: `contentView`, `returnedChars`, `isPartial`, `sourceChars` where available.
4. Warn before full-content reads for large files and suggest `startLine/endLine`, `matchString`, or `minify:"symbols"`.
5. Emit evidence level `exact-text`.

### `ghViewRepoStructure`

Role: browse a GitHub repository tree.

Best for:

- Remote repository orientation.
- Verifying exact path/case before file reads.
- Choosing whether to clone or inspect selected files.

Depth/alignment/quality:

- Depth: **8.0/10**
- Alignment: **8.4/10**
- Quality: **8.4/10**

Improvements:

1. Add `next.ghGetFileContent` for files.
2. Add deeper `next.ghViewRepoStructure` for directories.
3. Add `next.ghCloneRepo` when a tree is large or multi-file proof is likely.
4. Expose generated/vendor auto-exclusions if they apply.
5. Add extension histogram and emit evidence level `orientation`.

### `ghSearchRepos`

Role: discover GitHub repositories by owner, keyword, topic, language, license, popularity, or activity.

Best for:

- Finding candidate source repositories.
- Triage before tree/code search.

Depth/alignment/quality:

- Depth: **7.6/10**
- Alignment: **7.8/10**
- Quality: **8.0/10**

Improvements:

1. Add `next.ghViewRepoStructure`, `next.ghSearchCode`, and `next.ghCloneRepo` per repository.
2. Clarify search semantics: keywords are ANDed, topics are sparse, `match` controls searched fields.
3. Add optional repository quality score using archive status, pushed date, stars, license, and issue activity.
4. Add candidate-triage mode: official-looking, active, source-available, likely package repository.
5. Emit evidence level `orientation`.

### `ghSearchPullRequests`

Role: search GitHub PRs, or read one PR's files/diffs/reviews through a `content:{}` selector (metadata, body, changedFiles, patches, comments, reviews, commits).

Best for:

- Finding merged PRs touching a symbol or path.
- Understanding why code changed through review discussion.
- Reviewing selected PR patches without pulling all diffs.

Depth/alignment/quality:

- Depth: **8.7/10**
- Alignment: **8.7/10**
- Quality: **8.4/10**

Improvements:

1. Add archaeology recipes: merged PRs by symbol, regressions by date range.
2. Prefer selected `content` fields/patch ranges; warn when fetching all patches/comments.
3. Add next hints from changed files to `ghGetFileContent`, selected patch fetches, and `ghCloneRepo`.
4. Emit evidence level `historical-context`; exact patches can upgrade to `exact-text`.

### `ghSearchIssues`

Role: search GitHub issues, or read one issue through a `content:{}` selector (metadata, body, comments).

Best for:

- Triaging reported bugs and feature requests.
- Tracing a symptom to its discussion thread.
- Linking issues to the PRs that resolved them.

Depth/alignment/quality:

- Depth: **8.7/10**
- Alignment: **8.7/10**
- Quality: **8.4/10**

Improvements:

1. Add triage recipes: open issues by label, recently updated, linked-PR discovery.
2. Prefer selected `content` fields; warn when fetching all comments.
3. Add next hints to `ghSearchPullRequests` for resolving PRs.
4. Emit evidence level `historical-context`.

### `ghSearchCommits`

Role: walk a repository's commit history for a path/range (`path`, `since`/`until`, `branch`, `author`, `committer`, `includeDiff`), or compare two refs through `base`+`head` (returns aheadBy/behindBy/totalCommits + commit list). No sort/order.

Best for:

- Finding the first/last commit touching a path.
- Auditing changes in a date or ref range.
- Comparing two branches, tags, or SHAs.

Depth/alignment/quality:

- Depth: **8.7/10**
- Alignment: **8.7/10**
- Quality: **8.4/10**

Improvements:

1. Add recipes: first commit touching path, ref-vs-ref divergence, author history.
2. Prefer omitting `includeDiff` for listing; warn on large diff pulls.
3. Add next hints from commits to `ghGetFileContent` at SHA and `ghCloneRepo`.
4. Emit evidence level `historical-context`; exact diffs can upgrade to `exact-text`.

### `ghListReleases`

Role: list a repository's releases plus latest stable, with opt-in `includeAssets`. Opt-in tool (`ENABLE_RELEASES=true`).

Best for:

- Finding the latest stable release and tag.
- Mapping versions to release notes.
- Locating downloadable assets when needed.

Depth/alignment/quality:

- Depth: **8.7/10**
- Alignment: **8.7/10**
- Quality: **8.4/10**

Improvements:

1. Make `ENABLE_RELEASES=true` remediation explicit when disabled.
2. Keep `includeAssets` opt-in to bound output.
3. Add next hints from a release tag to `ghSearchCommits` (ref compare) and `ghGetFileContent` at tag.
4. Emit evidence level `historical-context`.

### `ghCloneRepo`

Role: clone/materialize a GitHub repository, branch, or sparse path to a local path for local tooling.

Best for:

- Escalating remote candidates to local search/content/LSP proof.
- Multi-file investigations.
- Working around GitHub search/indexing limitations.

Depth/alignment/quality:

- Depth: **8.6/10**
- Alignment: **9.0/10**
- Quality: **8.5/10**

Improvements:

1. Make `ENABLE_CLONE=true` remediation explicit when clone is disabled.
2. Add sparse checkout guidance for large repositories and known subdirectories.
3. Add full post-clone next chain: `localViewStructure`, `localFindFiles`, `localSearchCode`, then `lspGetSemantics` if language is supported.
4. Expose `cacheAge` and `forceRefresh` guidance.
5. Emit evidence level `orientation` bridge, not proof by itself.

### `npmSearch`

Role: search npm packages and resolve exact package names to metadata/source repositories.

Best for:

- Package lookup.
- Finding source repository for a dependency.
- Package health triage.

Depth/alignment/quality:

- Depth: **7.5/10**
- Alignment: **7.9/10**
- Quality: **8.2/10**

Improvements:

1. Add direct next hints when a GitHub repository is detected: `ghViewRepoStructure`, `ghSearchCode`, `ghCloneRepo`.
2. Add package health summary: weekly downloads, deprecated flag, last publish, license, types, repository detected.
3. Improve keyword result disambiguation: exact name match, official-looking scope, high-download candidate.
4. Add package-risk diagnostics for deprecated/unmaintained packages and typo-like names.
5. Emit evidence level `package-metadata`.

---

## Open improvements by priority

These are candidates ranked by impact, not commitments or dated plans. Priorities
describe relative ordering for contributors picking up work.

### P0

1. Standardize `next` hints for local search/content/LSP and GitHub clone flows.
2. Add structured diagnostics alongside free-form warnings.
3. Normalize local/LSP path fields.

### P1

1. Add evidence-level metadata across all tools.
2. Improve structural AST zero-match repair examples.
3. Add GitHub empty-result uncertainty diagnostics.
4. Add package or repository next hints.

### P2

1. Add extension histograms to tree/file discovery tools.
2. Add repository or package quality scoring.
3. Add cache-age reporting for clone/materialized content.

## Verification checklist for future changes

A change that claims to improve tool quality must run these commands:

```bash
yarn build
yarn typecheck
yarn lint
OCTOCODE_TRUST_PROJECT_LSP_CONFIG=1 yarn test
yarn workspace @octocodeai/octocode-engine check:rust
yarn workspace @octocodeai/octocode-engine lint:rust
yarn workspace @octocodeai/octocode-engine test:rust
yarn platforms:check
```

Then smoke every MCP tool through the built stdio server, with local tools and clone turned on.

Acceptance:

- MCP lists all expected tools.
- Every listed tool returns `isError !== true` for a representative query.
- Any generated benchmark timestamp/report noise is reverted unless the benchmark output itself changed intentionally.
