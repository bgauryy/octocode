# Octocode research manifest

Octocode research connects a question to inspectable code evidence. The agent chooses scope and evaluates evidence; tools retrieve source, syntax, repository topology, language-server results, and provider records. A search locates a candidate — it does not establish identity, completeness, or behavior.

This page owns tool selection: choosing and combining the 16 public tools for local, remote and history research. The concept and research loop are in [OCTOCODE_PROTOCOL.md](OCTOCODE_PROTOCOL.md); parameters are in the [tool reference](OCTOCODE_TOOLS.md); executable workflows are in the [research skill](../skills/octocode-research/SKILL.md); validation requirements are in the [contributor acceptance guide](../skills-dev/octocode-dev/docs/TOOL_QUALITY.md).

## Discover the contract before the query

Inspect the contract before calling (from the monorepo root, after building the CLI):

```bash
node packages/octocode/out/octocode.js scheme
node packages/octocode/out/octocode.js scheme localSearch --view query --compact
node packages/octocode/out/octocode.js scheme ghGetHistoryItem --view query
```

The catalog shows available tools and configuration gates; a compact schema shows fields, operation variants, and conditional relations. Read the full schema when a nested selector is abbreviated — for example, selected PR patches accept both file selection and added/deleted line ranges.

Execute with `TOOL_NAME 'JSON'` — one query object or a batch of up to five same-tool queries. Batch only independent work; sequence calls when a later query needs an identity, path, source line, snapshot, cursor, or continuation from an earlier one.

`mainGoal` and `reasoning` are optional decision context for multi-call research on an unknown (each batch row states its own); omit them on simple lookups. They are not ranking controls or proof. A `next.*` page or `hints.*` lead carries the brief only when the query that produced it sent one. Result `index` maps to the zero-based input position, and one row can fail while siblings succeed; `hints` (prose tips in `hints.text`, plus optional leads) suggest recovery or follow-ups, not result data. Do not mix fields across operations or assume a former tool name still aliases. Follow executable `next.*` pages across collection, content, diagnostic, and whole-response pagination — a first page, bounded scan, empty result, or bare cursor is not a completeness claim. See [How every tool call works](OCTOCODE_TOOLS.md#how-every-tool-call-works) for the full shared envelope.

## Choose the evidence surface

| Tool | Question it answers | Evidence boundary |
|---|---|---|
| `localSearch` | Where does this text or regex occur? | Lexical occurrence within the searched scope; literal, Rust regex (default) or PCRE2. |
| `structureSearch` | Which directories and files exist, by name or metadata? | Filesystem layout within the walked scope; no parsing. |
| `astSearch` | Which declarations, syntax trees, or structural matches are present? | Structural syntax within the scanned scope; comments and strings never match. |
| `astTopology` (beta) | Which files depend on one another, form paths and cycles, or are unreachable? | Syntactic file topology; unresolved imports, dynamic loading and excluded files limit coverage. |
| `astRewrite` (beta, CLI only) | How would a structural codemod change these files? | Preview first; apply only if files are unchanged since the preview. |
| `localFetch` | What does a known local file contain? | `none` (the default) preserves source apart from security redaction; transformed views are lossy. |
| `lspSearch` | Which definition, references, callers, or types does the server resolve? | Server and project scope limit semantic evidence. |
| `ghSearchRepo` / `ghSearchCode` / `ghStructure` | Which repositories, indexed code, or tree paths are candidates? | Code search uses GitHub's indexed default branch; `ghStructure` can select a ref. |
| `ghGetFileContent` | What is in a known remote file? | Pin the ref for reproducibility; file views and provider limits still apply. |
| `ghSearchHistory` | Which PRs, issues, or commits are candidates? | Discovery identifies records; it does not fetch every detail surface. |
| `ghGetHistoryItem` | What does a known PR, issue, commit, or comparison contain? | Selected detail can have independent list and content continuations. |
| `ghCloneRepo` (CLI only) | How can remote source become a local checkout? | Cloning supplies source; local analysis supplies proof. |
| `artifactSearch` | Which package metadata and source repository match the request? | Package metadata does not prove source behavior or installed-version equivalence. |
| `clasify` (needs a key) | Which unread candidates matter, where in a file is the answer, is this snippet enough? | A hint for what to read next; never proof of identity, safe deletion or absence. See [OCTOCODE_CLASIFY.md](OCTOCODE_CLASIFY.md). |

The separation follows the underlying interfaces: [ripgrep searches text](https://github.com/BurntSushi/ripgrep/blob/master/GUIDE.md), [AST patterns match syntax nodes](https://ast-grep.github.io/guide/rule-config/atomic-rule.html), and [LSP supplies document and workspace language features](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/).

Tool availability, a recognized extension, a parser fixture, and a running language server are separate facts — see the [language and feature reference](../packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md).

## Local workflow

Start at the cheapest step that resolves the missing evidence; a known path needs no repository-wide search.

1. **Orient when the area is unfamiliar.** `structureSearch` `operation:"tree"` (default) for layout, `operation:"files"` for names and metadata. Supply an absolute `path`; use `names` for file patterns and `extensions`/`entryType` for filtering.
2. **Locate an anchor.** `localSearch` has no `operation` field: set `searchText` and choose `regex:"literal"`, `"rust"` (default) or `"pcre2"` explicitly when the text has metacharacters. When code shape matters, use `astSearch` `operation:"match"` with exactly one nonblank `pattern` or `rule`; set `langType` for directory searches (a single file selects its grammar from the extension). `operation:"symbols"` gives a declaration outline; `operation:"syntaxTree"` pages one file's nodes (`nodeOffset`/`nodeLimit`/`namedOnly`).
3. **Read the relevant source.** `localFetch` with a returned line range or `matchString`; `path` alone is valid. Omitted `minify` means exact source. Choose `minify:"standard"` for compact source or `"symbols"` for an outline; security redaction still applies.
4. **Map topology when needed** (beta). `astTopology` `analysis:` `dependencies`, `dependents`, `path`, `cycles`, `reachability`, `deadCode`, or `drift`. Review diagnostics for skipped files, unresolved edges and bounded results, and keep `entrypoints`, `includeTests`, exclusions and caps fixed when comparing runs.
5. **Resolve identity when needed.** `lspSearch` with an observed `uri` and either `symbolName` plus 1-based `lineHint` (`orderHint` picks among repeats on that line) or a zero-based UTF-16 `position`. An `astSearch` `symbols` row's `name`+`line`, or an identifier capture's `text`+`line`, is `symbolName`+`lineHint` as-is. `documentSymbols` and `diagnostic` need only `uri`; `workspaceSymbol` needs `symbolName` plus `uri` or `workspaceRoot`. Re-read and re-anchor when the tool reports drift.
6. **Validate the conclusion.** Read callers and imports, check lexical wiring outside the language project, and run affected tests and the real CLI, MCP or build path before deleting code or asserting changed behavior.

Graph analysis selects candidates; it does not prove safe deletion or runtime reachability. AST patterns establish shape, not server-resolved identity, and a zero-match pattern can mean a grammar or pattern mismatch. An empty LSP result can reflect server capability or project scope; a syntactic fallback does not establish cross-file identity. None of these proves no usage. Investigate local runtime behavior against the installed dependency version: inspect installed package metadata and entrypoints if access rules permit, otherwise the lockfile and in-scope source; do not substitute the upstream default branch without checking the relationship.

## External workflow

For an unknown repository, start with `artifactSearch` or `ghSearchRepo`. Package queries require an ecosystem `type` and exactly one of `packageName` or `keywords` (PyPI is exact-only); keyword discovery uses opaque `cursor` and `pageSize` — copy the complete `next.nextPage`. Preserve the repository subdirectory for monorepo packages.

Use `ghStructure` for layout and path case, and `ghSearchCode` for indexed candidates (`match:"path"` searches paths, `match:"file"` searches content). Snippets can be transformed — not an exact-source substitute — and an empty result does not prove absence on another branch or outside the provider index.

Read a selected path with `ghGetFileContent`; supply an observed commit SHA in `branch` for revision-dependent claims and record the resolved identity, since a branch name can move. Fetch exact source before quoting a snippet or interpreting a diff in isolation: use `minify:"none"` and preserve the returned `commitSha`, and match or line metadata. `standard` and `symbols` are transformed views and do not prove omitted text was absent.

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
| Default CLI output | Single-line JSON; a shared path prefix is hoisted to top-level `base` (`--pretty` indents). | Resolve relative paths against `base` before interpreting rows. |
| Character window | Bound the selected or transformed content. | Follow returned continuations; offsets are not source-line numbers. |

File reads expose `none`, `standard`, and `symbols`. History is operation-specific: PR detail exposes `none`/`standard`; discovery, issue detail, and commit/compare accept no `minify`, and history has no `symbols` mode — do not send file-read modes to an operation that rejects them. See the [tool reference](OCTOCODE_TOOLS.md) for defaults, match preservation, fallback behavior, and window semantics. Equal field names do not imply local/remote equivalence, and a minification extension entry is not evidence of a structural grammar, outline extractor, graph resolver, or installed LSP server.

## Follow the complete continuation contract

Inspect every result row: `status` (`empty` and `error` are different outcomes), `data`, `next` pages, warnings, and `hints`. Responses are minimal by default; `debug:true` adds the fields the contract classes verbose: `meta` (evidence kind, diagnostics), scan stats, receipts, and echoes. Use returned executable queries rather than reconstructing paths from display strings.

For each partial surface, execute its `next.*` page with the supplied tool and query; `hints.*` leads are optional. Do not advance every counter together or compute the next offset from the requested character length; semantic chunking can expand a window to a boundary, and page counters can be estimates while the continuation offset is authoritative. Whole-response continuations carry a snapshot; when the result set changes, the tool returns a restart with an offset-zero query, and earlier pages must be discarded.

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
