# Octocode research manifest

Octocode research connects a question to inspectable code evidence. The agent chooses scope and evaluates evidence; tools retrieve source, syntax, repository topology, language-server results, and provider records. A search locates a candidate — it does not establish identity, completeness, or behavior.

This page covers choosing and combining the 12 public tools. See the [tool reference](OCTOCODE_TOOLS.md) for parameters, the [research skill](../skills/octocode-research/SKILL.md) for executable workflows, and the [contributor acceptance guide](MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md) for validation requirements.

## Discover the contract before the query

Inspect the contract before calling (from the monorepo root, after building the CLI):

```bash
node packages/octocode/out/octocode.js scheme --compact
node packages/octocode/out/octocode.js scheme localSearch --view query --compact
node packages/octocode/out/octocode.js scheme ghGetHistoryItem --view query
```

The catalog shows available tools and configuration gates; a compact schema shows fields, operation variants, and conditional relations. Read the full schema when a nested selector is abbreviated — for example, selected PR patches accept both file selection and added/deleted line ranges.

Execute with `TOOL_NAME 'JSON' --compact` — one query object or a batch of up to five same-tool queries. Batch only independent work; sequence calls when a later query needs an identity, path, source line, snapshot, cursor, or continuation from an earlier one.

Optional `goal` and `reasoning` are short decision context, not ranking controls or proof. Result `index` maps to the zero-based input position, and one row can fail while siblings succeed; `hints` suggest recovery, not result data. Do not mix fields across operations or assume a former tool name still aliases. Follow executable `next.*` calls across collection, content, diagnostic, and whole-response pagination — a first page, bounded scan, empty result, or bare cursor is not a completeness claim. See [How every tool call works](OCTOCODE_TOOLS.md#how-every-tool-call-works) for the full shared envelope.

## Choose the evidence surface

| Tool | Question it answers | Evidence boundary |
|---|---|---|
| `localSearch` | Where is text, syntax, a path, or a directory entry? | Text proves occurrence; AST proves syntax within the searched scope. |
| `localFetch` | What does a known local file contain? | `none` preserves selected source apart from security redaction; transformed views are lossy. |
| `astSearch` | Which files depend on one another or form paths and cycles? | Syntactic file topology; unresolved imports and excluded files limit coverage. |
| `lspSearch` | Which definition, references, callers, or types does the server resolve? | Server and project scope limit semantic evidence. |
| `ghSearch` | Which indexed code, repositories, or tree paths are candidates? | Code search uses GitHub's indexed default branch; a tree query can select a ref. |
| `ghGetFileContent` | What is in a known remote file? | Pin the ref for reproducibility; file views and provider limits still apply. |
| `ghSearchHistory` | Which PRs, issues, or commits are candidates? | Discovery identifies records; it does not fetch every detail surface. |
| `ghGetHistoryItem` | What does a known PR, issue, commit, or comparison contain? | Selected detail can have independent list and content continuations. |
| `ghCloneRepo` | How can remote source become a local checkout? | Cloning supplies source; local analysis supplies proof. |
| `artifactSearch` | Which package metadata and source repository match the request? | Package metadata does not prove source behavior or installed-version equivalence. |

Tool availability, a recognized extension, a parser fixture, and a running language server are separate facts — see the [language and feature reference](../packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md).

## Local workflow

Start at the cheapest step that resolves the missing evidence; a known path needs no repository-wide search.

1. **Orient when the area is unfamiliar.** `astSearch` `operation:"tree"` for layout, `operation:"files"` for names and metadata. Supply an absolute `path`; use `names` for file patterns and `namePattern` for tree filtering.
2. **Locate an anchor.** `localSearch` `searchText` for identifiers, messages, and literals (choose the regex mode explicitly). Use `astSearch(operation:"match")` with exactly one of `pattern` or `rule` for a syntax shape.
3. **Read the relevant source.** `localFetch` with a returned line range or `matchString`; set `minify:"none"` when quoting or examining precise syntax. An outline helps find declarations before reading bodies.
4. **Map topology when needed.** Graph `dependencies`, `dependents`, `path`, `cycles`, `reachability`, or `deadCode`; review diagnostics for skipped files, unresolved edges, and bounded results.
5. **Resolve identity when needed.** `lspSearch` with a real `uri`, `symbolName`, and `lineHint` for anchored operations. `documentSymbols` and `diagnostic` are per-document; `workspaceSymbol` searches the server workspace without a symbol line.
6. **Validate the conclusion.** Read callers and imports, check lexical wiring outside the language project, and run affected tests before deleting code or asserting changed behavior.

AST patterns establish shape, not server-resolved identity; a zero-match pattern can mean a grammar or pattern mismatch. An empty LSP result can reflect server capability or project scope. Neither proves no usage. Graph dead-code results are candidates, especially where imports cannot resolve. Investigate local runtime behavior against the installed dependency version — inspect installed package metadata and entrypoints if access rules permit, otherwise the lockfile and in-scope source; do not substitute the upstream default branch without checking the relationship.

## External workflow

For an unknown repository, start with `artifactSearch` or `ghSearch(operation:"repositories")`. Package queries require an ecosystem `type` and exactly one of `packageName` or `keywords` (PyPI is exact-only); keyword discovery uses opaque `cursor` and `pageSize` — copy the complete `next.nextPage`. Preserve the repository subdirectory for monorepo packages.

Use `ghSearch(operation:"tree")` for layout and path case, and `operation:"code"` for indexed candidates (`match:"path"` searches paths, `match:"file"` searches content). Snippets can be transformed — not an exact-source substitute — and an empty result does not prove absence on another branch or outside the provider index.

Read a selected path with `ghGetFileContent`; supply an observed commit SHA in `branch` for revision-dependent claims and record the resolved identity, since a branch name can move. Fetch exact source before quoting a snippet or interpreting a diff in isolation: use `minify:"none"` and preserve the returned `resolvedBranch`, commit identity, and match or line metadata. `standard` and `symbols` are transformed views and do not prove omitted text was absent.

For repeated reads, AST queries, graph analysis, or semantic verification, use `ghCloneRepo` then the local tools on its returned `localPath` (`sparsePath` narrows the checkout). Cloning requires persistent storage and its availability gate — inspect catalog diagnostics rather than assuming it is enabled. A fresh clone reports verification for its checkout; a cache reuse can report `verified:false`, so a cached path plus HEAD identity does not prove the working tree is unchanged. A sparse clone is complete only within its subtree. Materialization installs no dependencies or language servers, and reading or cloning source does not authorize executing it.

## History workflow

`ghSearchHistory` uses singular operations `pullRequest`, `issue`, or `commit`. PR discovery can be global; issue and commit queries require `owner` and `repo`; commit discovery supports path, time, author, and branch constraints. Fields are not interchangeable across operations. Pass the returned identity to `ghGetHistoryItem`:

| Operation | Identity | Detail selection |
|---|---|---|
| `pullRequest` | `owner`, `repo`, `number` | `content` selects body, files, patches, comments, reviews, and commits. |
| `issue` | `owner`, `repo`, `number` | `content` selects body and discussion comments. |
| `commit` | `owner`, `repo`, `ref` | `includeDiff` requests patches; `path` can narrow files. |
| `compare` | `owner`, `repo`, `base`, `head` | `includeDiff` requests patches; commit and file pages are separate. |

For a PR, request only the surfaces the question needs; selected patches use `mode:"selected"` with `files` or `ranges` (read the schema for the range object). Follow each continuation independently — finishing the changed-file list does not finish a long patch, PR body, comment body, review collection, or commit list; preserve selectors and immutable identities. Provider-omitted patches and terminal caps persist after reachable pages are consumed. Review comments explain intent; source at the relevant revision establishes implementation.

## Content views and smart output

`minify`, `concise`, schema compaction, semantic chunking, and response pagination are separate controls.

| Control | Purpose | How to use the result |
|---|---|---|
| `minify:"none"` | Preserve selected source or supported exact history content, apart from security redaction. | Use for quotes, syntax, comments, and diff evidence. |
| `minify:"standard"` | Reduce content with a file- or surface-specific transformation. | Inspect the effective view; do not infer that removed text was absent in source. |
| `minify:"symbols"` | Extract a file outline where supported. | Use line anchors to read bodies; it is not a complete source view. |
| `concise` | Select a smaller discovery payload where the operation supports it. | Inspect the operation schema; it is not a universal minification flag. |
| `--compact` | Reduce CLI envelope and repeated metadata. | Resolve `base` and top-level `shared` values before interpreting rows. |
| Character window | Bound the selected or transformed content. | Follow returned continuations; offsets are not source-line numbers. |

File reads expose `none`, `standard`, and `symbols`. History is operation-specific: PR detail exposes `none`/`standard`; discovery, issue detail, and commit/compare accept no `minify`, and history has no `symbols` mode — do not send file-read modes to an operation that rejects them. See the [tool reference](OCTOCODE_TOOLS.md) for defaults, match preservation, fallback behavior, and window semantics. Equal field names do not imply local/remote equivalence, and a minification extension entry is not evidence of a structural grammar, outline extractor, graph resolver, or installed LSP server.

## Follow the complete continuation contract

Inspect every result row — `status`, `meta.evidence`, `meta.diagnostics`, and the tool's `data`. Public response shaping removes free-form `warnings`; rely on typed diagnostics and effective content metadata. Compact output hoists shared values and shortens displayed paths — use returned executable queries rather than reconstructing paths from display strings.

For each partial surface, execute its `next.*` call with the supplied tool and query. Do not advance every counter together or compute the next offset from the requested character length; semantic chunking can expand a window to a boundary, and page counters can be estimates while the continuation offset is authoritative.

| Surface | Independent bounds to inspect |
|---|---|
| Local discovery | File, match, traversal, and result limits. |
| File reads | Selected source lines and transformed-content characters. |
| Graph | Result rows, nested topology, and diagnostics. |
| LSP | Result pages, snapshots, hierarchy depth, and server coverage. |
| GitHub discovery | Search pages, tree scope, metadata pages, and provider limits. |
| History | Records, files, comments, reviews, commits, bodies, and patches. |
| npm discovery | Keyword-result pages and registry limits. |
| Response text | Envelope-level text windows, separate from underlying tool data. |

The acceptance contract requires a schema-valid executable continuation for reachable partial data, or an explicit terminal-limit diagnostic when the bound cannot be continued — a testing requirement, not proof that a response or provider is complete. Repeating content, missing continuations, or unreachable offsets are defects to reproduce and report.

## State what the evidence establishes

Record the claim, source path and revision, evidence type, traversed scope, and remaining uncertainty. Source paths and line numbers support code claims; PR and commit identities support history claims; a transformed view needs an exact read before it supports a quote.

Stop when the question has sufficient evidence; extend the investigation when a coverage gap changes the decision. A provider cap, unavailable language server, unresolved graph edge, or excluded file prevents a universal absence claim, but does not require repeating the same unproductive query.
