# Agent workflow and tool improvement notes

This page gives the recommended agent workflow for Octocode tools, the response
contracts that all tools share, and the open improvement notes per tool. It is
written for contributors and for agents tuning their own tool use.

To exercise the full tool surface locally, start the built MCP stdio server:

```bash
ENABLE_LOCAL=true \
ENABLE_CLONE=true \
OCTOCODE_TRUST_PROJECT_LSP_CONFIG=1 \
node packages/octocode-mcp/dist/index.js
```

The discovery catalog contains 10 tools, with 10 enabled by default.

## Historical baseline: 2026-08-30 contract audit

This table is retained for change context and is superseded by the
[current 2026-09-01 scorecard](https://github.com/bgauryy/octocode/blob/main/docs/MCP_CLI_TOOL_CONTRACT_GAPS.md).

Scores combine schema clarity, response boundedness, workflow alignment, and exercised behavior. They are comparative routing scores, not claims that every backlog item is complete.

| Tool | Score | Best workflow role |
|---|---:|---|
| `localSearch` | 9.5 | unified text, AST, path, metadata, and tree discovery |
| `localGetFileContent` | 9.4 | bounded exact local evidence |
| `lspGetSemantics` | 9.2 | semantic identity and impact proof |
| `localAnalyzeGraph` | 9.1 | bounded file-topology questions |
| `ghSearch` | 8.9 | unified remote code, repository, and tree discovery |
| `ghSearchHistory` | 8.9 | PR, issue, and commit discovery |
| `ghGetHistoryItem` | 8.8 | exact PR, issue, commit, and ref-comparison detail |
| `ghGetFileContent` | 8.8 | exact remote evidence |
| `ghCloneRepo` | 8.8 | remote-to-local escalation |
| `npmSearch` | 8.6 | package-to-source routing |

Audit changes accepted: `localSearch`, `ghSearch`, and the history search/get pair replace overlapping
overlapping public tools with strict operation variants. The superseded interfaces
are no longer registered by CLI or MCP. Compact and full schemas
expose conditional `relations` and variants; every operation has a valid command
pattern; continuations preserve the selected operation; and CLI and MCP share the
same runners. Graph pagination and evidence rules are unchanged.

## Standard agent workflow

Use this workflow by default. It keeps cost low, raises proof quality step by step, and avoids treating discovery as proof.

```text
1. Orient
   localSearch(operation:"tree") / ghSearch(operation:"tree"|"repositories") / npmSearch

2. Discover candidates
   localSearch(operation:"files"|"text"|"structural") / ghSearch(operation:"code") / ghSearchHistory

3. Inspect exact evidence
   localGetFileContent / ghGetFileContent

4. Map repository topology when the question requires it
   localAnalyzeGraph for dependencies, dependents, paths, cycles, reachability, or dead-code candidates

5. Prove symbol identity or impact
   lspGetSemantics for definitions, references, hover, callers/callees, diagnostics

6. Escalate remote to local when needed
   ghCloneRepo -> localSearch -> localGetFileContent -> lspGetSemantics

7. Synthesize with evidence status
   Mark every claim as: orientation, candidate, exact text, semantic proof, or historical proof.
```

### Default local-code workflow

```text
localSearch(operation="tree", path)
  -> localSearch(operation="files", path, names/extensions/metadata)
  -> localSearch(operation="text"|"structural", path, query)
  -> localGetFileContent(path, line range or matchString)
  -> localAnalyzeGraph(operation) when repository topology matters
  -> lspGetSemantics(uri, symbolName, lineHint)
```

Rules:

- Use `localSearch(operation:"tree")` first for unknown directories.
- Use `localSearch(operation:"text", resultView:"discovery")` when snippets are not needed.
- Use `localSearch(operation:"structural")` for code shape.
- If structural `pattern` returns zero, try a YAML `rule`.
- Use `localGetFileContent(minify:"symbols")` before reading large files.
- Use graph `dependents`/`path` for file impact, `cycles`/`reachability` for architecture, and `deadCode` only as a candidate generator.
- Treat graph import edges as syntactic evidence; use LSP for symbol identity.
- Use LSP only after anchoring a real symbol line from search/symbols.

Graph operation routing:

| Question | Operation | Required operation fields |
|---|---|---|
| imports of a file | `dependencies` | `file`; optional `depth` |
| importers of a file | `dependents` | `file`; optional `depth` |
| shortest directed import route | `path` | `file`, `target` |
| strongly connected file components | `cycles` | none beyond `path` root |
| entrypoint reachability | `reachability` | optional `entrypoints`, `includeTests` |
| dead-code candidates | `deadCode` | optional `entrypoints`, `includeTests` |

### Default GitHub workflow

```text
ghSearch(operation:"repositories") / npmSearch
  -> ghSearch(operation:"tree")
  -> ghSearch(operation:"code", match:"path" for filenames or match:"file" for snippets)
  -> ghGetFileContent(range/matchString/symbols)
  -> ghCloneRepo when cross-file or LSP proof is needed
  -> local tools on cloned localPath
```

Rules:

- Do not treat empty GitHub search as proof of absence; GitHub is indexed/default-branch-limited.
- Use `ghSearch(operation:"tree")` to verify path case before `ghGetFileContent`.
- Use `ghCloneRepo` for multi-file reasoning, large investigations, or semantic proof.

### Default history workflow

```text
ghSearchHistory(operation:"commits", owner, repo, path?)
  -> ghGetHistoryItem(operation:"commit", owner, repo, ref)
  -> ghSearchHistory(operation:"pullRequests", owner, repo, keywords)
  -> ghGetHistoryItem(operation:"pullRequest", owner, repo, number, content)
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
  -> ghSearch(operation:"tree"|"code")
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
| `localSearch` hit | `localGetFileContent`, `lspGetSemantics` when a symbol anchor is available. |
| `localSearch` file result | `localGetFileContent`, or another scoped `localSearch` operation. |
| `localSearch` directory result | deeper `operation:"tree"`, then `files`, `text`, or `structural`. |
| `localAnalyzeGraph` topology result | `localGetFileContent` around exact imports; `lspGetSemantics` when symbol identity matters. |
| `localAnalyzeGraph` dead-code candidate | `localGetFileContent`, LSP references excluding declaration, broad search, tests/build. |
| `ghSearch(operation:"code")` hit | `ghGetFileContent`, `ghSearch(operation:"tree")`, `ghCloneRepo`. |
| `ghSearch(operation:"repositories")` repository | `ghSearch(operation:"tree"|"code")`, `ghCloneRepo`. |
| `ghSearch(operation:"tree")` file | `ghGetFileContent`. |
| `ghSearch(operation:"tree")` directory | deeper `ghSearch(operation:"tree")`, `ghCloneRepo`. |
| `ghGetFileContent` file | `ghCloneRepo` for multi-file/LSP work. |
| `ghCloneRepo` result | `localSearch`, then exact reads or LSP proof. |
| `npmSearch` GitHub repository | `ghSearch(operation:"tree"|"code")`, `ghCloneRepo`. |
| `ghSearchHistory` candidate | `ghGetHistoryItem` with the returned operation-specific identity. |
| `ghGetHistoryItem` changed file or commit | `ghGetFileContent` at ref, selected detail follow-up, `ghCloneRepo`. |
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
| `localSearch` | `orientation` | `localSearch` or `localGetFileContent`. |
| `localSearch` | `candidate` | `localGetFileContent` or `localSearch`. |
| `localSearch` text hit | `exact-text` for snippet presence, not symbol identity | `lspGetSemantics`. |
| `localSearch` structural hit | `candidate` to `exact-text` depending row | `localGetFileContent`, then `lspGetSemantics`. |
| `localGetFileContent` | `exact-text` | `lspGetSemantics` for identity/impact. |
| `localAnalyzeGraph` | `candidate` file-topology evidence | exact import reads; LSP for symbol identity; tests/build before deletion. |
| `lspGetSemantics` | `semantic-proof` | read reference/caller lines for quotes. |
| `ghSearch(operation:"repositories")` | `orientation` | `ghSearch(operation:"tree"|"code")`. |
| `npmSearch` | `package-metadata` | GitHub tools or clone. |
| `ghSearch(operation:"code")` | `candidate` or `exact-text` snippet | `ghGetFileContent`/`ghCloneRepo`. |
| `ghSearch(operation:"tree")` | `orientation` | `ghGetFileContent`/`ghCloneRepo`. |
| `ghGetFileContent` | `exact-text` | clone for local proof. |
| `ghSearchHistory` | `historical-context` candidate | `ghGetHistoryItem`. |
| `ghGetHistoryItem` | `historical-context` or exact diff | fetch exact files at ref or clone for local proof. |
| `ghCloneRepo` | `orientation` bridge | local tools. |

Acceptance criteria:

- Agents can distinguish "found a candidate" from "proved identity".
- Every non-proof result includes an upgrade suggestion.
- Search relevance order is never presented as proof.

---

## Per-tool documentation and improvements

### `localSearch`

Role: route all local discovery through one strict operation: text, structural, files, or tree.

Best for:

- Finding candidate code locations and paths.
- Fast repository-wide text search.
- Structural code-shape discovery.
- Producing anchors for exact reads and LSP proof.

Use when:

- You know a symbol, phrase, import, error text, or code shape.
- You need candidate files/lines before reading content.

Avoid when:

- You only need directory shape: use `operation:"tree"`.
- You only need file metadata/name filters: use `operation:"files"`.
- You already need semantic identity: use `lspGetSemantics` after anchoring.

Depth/alignment/quality:

- Depth: **9.5/10**
- Alignment: **9.6/10**
- Quality: **9.0/10**

Current strengths:

- Native ripgrep and structural engine.
- Detailed stats and pagination.
- Four discriminated operations with operation-specific fields.
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

- You have no anchor yet: first run `documentSymbols` or `localSearch`.
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

- You only need file discovery: use `localSearch(operation:"files")`.
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

### `localSearch(operation:"files")`

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
4. Add `next.localSearch` scoped to matched directories.
5. Emit evidence level `candidate`.

### `localSearch(operation:"tree")`

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

### `localAnalyzeGraph`

Role: answer bounded repository file-graph questions through one operation-discriminated tool.

Best for:

- Traversing dependencies or dependents and finding the shortest import path.
- Finding strongly connected file components and entrypoint reachability.
- Producing repository-wide dead-code candidates for semantic verification.

Depth/alignment/quality:

- Depth: **9.0/10**
- Alignment: **9.4/10**
- Quality: **9.1/10**

Current strengths:

- One lean public surface covers six related graph operations without an unbounded graph query language.
- Full JSON Schema preserves operation-specific required fields.
- Results are paginated and nested paths/components are capped with truncation metadata.
- Syntactic confidence and LSP verification hints prevent dead-code candidates from becoming deletion verdicts.

Improvements:

1. Keep the compact `relations` contract and all six operation examples covered by tests.
2. Keep entrypoint summaries bounded and retain count/truncation metadata.
3. Reuse one graph build across batched operations sharing path/exclusions/file cap.
4. Expand edge provenance beyond static/dynamic imports to named/star re-exports, calls, and containment where available.
5. Add a continuation for truncated SCC/dead-cluster members only when real workflows require the full component.

### `ghSearch`

Role: discover GitHub code or paths, repositories, and known repository trees
through strict `operation:"code"|"repositories"|"tree"` queries.

Best for:

- Remote candidate discovery and repository triage.
- Tree orientation before exact reads or clones.
- Mixed bulk discovery while retaining per-query operation and order.

Improvements:

1. Keep operation-specific relations, variants, and command examples synchronized with the exact schema.
2. Preserve the selected operation in every continuation.
3. Keep GitHub indexing uncertainty explicit on empty code-search results.
4. Emit evidence level `candidate` for code rows and `orientation` for repository/tree rows.
5. Keep code, repository, and tree behavior covered by field-for-field migration tests.

### `ghGetFileContent`

Role: read exact GitHub file content or materialize a directory subtree.

Best for:

- Remote exact evidence.
- Reading line ranges or match slices from GitHub.
- Verifying paths found by `ghSearch(operation:"code"|"tree")`.

Depth/alignment/quality:

- Depth: **8.5/10**
- Alignment: **8.8/10**
- Quality: **8.5/10**

Improvements:

1. Add stronger 404 recovery diagnostics: verify branch, path case, and tree with `ghSearch(operation:"tree")`.
2. Add `next.ghCloneRepo` when a file read suggests cross-file investigation.
3. Align content metadata with `localGetFileContent`: `contentView`, `returnedChars`, `isPartial`, `sourceChars` where available.
4. Warn before full-content reads for large files and suggest `startLine/endLine`, `matchString`, or `minify:"symbols"`.
5. Emit evidence level `exact-text`.

### `ghSearchHistory`

Role: search PR, issue, or commit candidates through the strict plural operations `pullRequests`, `issues`, and `commits`.

Best for:

- Finding merged PRs or issue threads by title and filters.
- Finding commits touching a path or matching a time/author constraint.
- Producing stable identities for `ghGetHistoryItem`.

Depth/alignment/quality:

- Depth: **8.7/10**
- Alignment: **8.7/10**
- Quality: **8.4/10**

Improvements:

1. Add recipes for merged-PR archaeology, issue triage, and commits touching a path.
2. Preserve the operation in every continuation and reject cross-operation fields.
3. Add runnable `ghGetHistoryItem` hints using `number` or `ref` from each result.
4. Emit evidence level `historical-context` candidate.

### `ghGetHistoryItem`

Role: fetch a known PR or issue by `number`, a commit by `ref`, or compare `base` and `head`.

Best for:

- Understanding why code changed through selected PR detail.
- Reading issue body/comments or a known commit diff.
- Comparing two branches, tags, or SHAs.

Depth/alignment/quality:

- Depth: **8.7/10**
- Alignment: **8.7/10**
- Quality: **8.4/10**

Improvements:

1. Prefer selected PR content and omit commit diffs until the identity is known.
2. Add next hints from changed files/refs to `ghGetFileContent` and `ghCloneRepo`.
3. Reject identities from another operation instead of coercing them.
4. Emit `historical-context`; exact patches and diffs can upgrade to `exact-text`.

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
3. Add a post-clone `localSearch` chain with explicit operations, then `lspGetSemantics` when supported.
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

1. Add direct next hints when a GitHub repository is detected: `ghSearch(operation:"tree"|"code")`, `ghCloneRepo`.
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
