# Octocode research manifest

Octocode research connects a question to inspectable code evidence. The agent
chooses the scope and evaluates the evidence; tools retrieve source, syntax,
repository topology, language-server results, and provider records. A successful
search locates a candidate. It does not by itself establish identity, completeness,
or behavior.

This page explains how to choose and combine the ten public tools. Use the
[tool reference](OCTOCODE_TOOLS.md) for parameters, the
[research skill](https://github.com/bgauryy/octocode/blob/main/skills/octocode-research/SKILL.md) for executable workflows,
and the [contributor acceptance guide](MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md)
for validation requirements.

## Discover the contract before the query

Run these commands from the monorepo root after building the CLI:

```bash
node packages/octocode/out/octocode.js tools --json
node packages/octocode/out/octocode.js tools localSearch --scheme --json --compact
node packages/octocode/out/octocode.js tools ghGetHistoryItem --scheme --json
```

The catalog shows available tools and configuration gates. A compact schema
exposes fields, operation variants, and conditional relations. Read the full
schema when a nested selector is abbreviated: for example, selected PR patches
support both file selection and added/deleted line ranges.

Raw execution uses `tools TOOL_NAME --queries 'JSON' --compact`; replace
`TOOL_NAME` with a catalog name and `JSON` with an object or a batch of up to five
query objects. Batch independent queries for the same tool. Sequence calls when
one needs an identity, path, source line, or continuation from another.

Queries can include the optional `goal` and `reasoning` fields shown in the full
schema. Result `index` identifies the corresponding query. Do not add fields
from another operation or assume a former tool name remains an alias.

## Choose the evidence surface

| Tool | Question it answers | Evidence boundary |
|---|---|---|
| `localSearch` | Where is text, syntax, a path, or a directory entry? | Text proves occurrence; AST proves syntax within the searched scope. |
| `localGetFileContent` | What does a known local file contain? | `none` preserves selected source apart from security redaction; transformed views are lossy. |
| `localAnalyzeGraph` | Which files depend on one another or form paths and cycles? | Syntactic file topology; unresolved imports and excluded files limit coverage. |
| `lspGetSemantics` | Which definition, references, callers, or types does the server resolve? | Server and project scope limit semantic evidence. |
| `ghSearch` | Which indexed code, repositories, or tree paths are candidates? | Code search uses GitHub's indexed default branch; a tree query can select a ref. |
| `ghGetFileContent` | What is in a known remote file or directory snapshot? | Pin the ref for reproducibility; file views and provider limits still apply. |
| `ghSearchHistory` | Which PRs, issues, or commits are candidates? | Discovery identifies records; it does not fetch every detail surface. |
| `ghGetHistoryItem` | What does a known PR, issue, commit, or comparison contain? | Selected detail can have independent list and content continuations. |
| `ghCloneRepo` | How can remote source become a local checkout? | Cloning supplies source; local analysis supplies proof. |
| `npmSearch` | Which package metadata and source repository match the request? | Package metadata does not prove source behavior or installed-version equivalence. |

Tool availability, a recognized extension, a parser fixture, and a running
language server are separate facts. The
[language and feature reference](https://github.com/bgauryy/octocode/blob/main/packages/octocode-engine/docs/SUPPORTED_LANGUAGES_AND_FEATURES.md)
separates structural grammars, outlines, graph extraction, LSP routing, and
minification configuration.

## Local workflow

Start at the cheapest step that resolves the missing evidence. A known path does
not need another repository-wide search.

1. **Orient when the area is unfamiliar.** Use `localSearch` with `operation:"tree"`
   for layout or `operation:"files"` for names and metadata. Supply an absolute
   `path`; use `names` for file patterns and `namePattern` for tree filtering.
2. **Locate an anchor.** Use `operation:"text"` with `searchText` for identifiers,
   messages, and literals. Choose the regex mode explicitly when interpretation
   matters. Use `operation:"structural"` with exactly one of `pattern` or `rule`
   when the question concerns a syntax shape.
3. **Read the relevant source.** Use `localGetFileContent` with a returned line
   range or `matchString`. Set `minify:"none"` when quoting or examining precise
   syntax. An outline helps identify declarations before reading their bodies.
4. **Map topology when needed.** Use graph `dependencies`, `dependents`, `path`,
   `cycles`, `reachability`, or `deadCode`. Review diagnostics for skipped files,
   unresolved edges, and bounded results.
5. **Resolve identity when needed.** Use `lspGetSemantics` with a real `uri`,
   `symbolName`, and `lineHint` for anchored operations. `documentSymbols` and
   `diagnostic` operate on a document; `workspaceSymbol` searches the server's
   workspace without requiring a symbol line.
6. **Validate the conclusion.** Read relevant callers and imports, check lexical
   wiring outside the language project, and run affected tests before deleting
   code or asserting changed behavior.

AST patterns establish shape, not server-resolved identity. A zero-match pattern
can indicate a grammar or pattern mismatch. An empty LSP result can indicate
server capability or project scope. Neither is sufficient evidence of no usage.
Graph dead-code results are candidates, especially where imports cannot resolve.

Use the installed dependency version when investigating local runtime behavior.
If repository access rules permit reading installed packages, inspect their
metadata and entrypoints. Otherwise, use the lockfile and source version evidence
available within the authorized scope. Do not substitute the upstream default
branch for the installed version without checking the relationship.

## External workflow

When a repository is unknown, start with `npmSearch` or `ghSearch(operation:"repositories")` repository
discovery. For npm, choose exactly one of `packageName` or `keywords`; pagination
applies to keyword discovery. Preserve the returned repository subdirectory when
the package lives in a monorepo.

Use `ghSearch(operation:"tree")` to establish layout and path case. Use
`operation:"code"` for indexed candidates: `match:"path"` searches paths and
`match:"file"` searches file content. Code-search snippets can be transformed and
are not an exact-source substitute. An empty result does not prove absence on a
different branch or outside the provider index.

Read a selected path with `ghGetFileContent`. Supply an observed commit SHA in
`branch` when the claim depends on a fixed revision. Record the resolved identity;
a branch name can move between calls. Fetch exact source before quoting a search
snippet or interpreting a diff in isolation.

For repeated reads, AST queries, graph analysis, or semantic verification, use
`ghCloneRepo` and then the local tools on its returned path. A sparse checkout or
`ghGetFileContent(type:"directory")` can materialize a smaller scope. These
operations require persistent storage and their configured availability gates.
Inspect catalog diagnostics rather than assuming cloning is enabled.

Materialization does not install dependencies or language servers, and a sparse
scope can omit files required for resolution. Reading or cloning external source
does not itself authorize executing that source.

## History workflow

Use `ghSearchHistory` with plural operations: `pullRequests`, `issues`, or
`commits`. PR discovery can be global; issue and commit queries require `owner`
and `repo`. Commit discovery supports path, time, author, and branch constraints.
Fields belonging to one operation are not interchangeable with another.

Pass the returned identity to `ghGetHistoryItem`:

| Operation | Identity | Detail selection |
|---|---|---|
| `pullRequest` | `owner`, `repo`, `number` | `content` selects body, files, patches, comments, reviews, and commits. |
| `issue` | `owner`, `repo`, `number` | `content` selects body and discussion comments. |
| `commit` | `owner`, `repo`, `ref` | `includeDiff` requests patches; `path` can narrow files. |
| `compare` | `owner`, `repo`, `base`, `head` | `includeDiff` requests patches; commit and file pages are separate. |

For a PR, first request the surfaces needed to answer the question. Selected
patches can use `mode:"selected"` with `files` or `ranges`; read the full schema
for the range object. Do not request all comments, commits, and patches merely
because they are available.

Follow each returned continuation independently. Finishing the changed-file list
does not finish a long patch, PR body, comment body, review collection, or commit
list. Preserve selectors and immutable identities from continuation queries.
Provider-omitted patches and terminal caps remain limitations after reachable
pages are consumed. Review comments explain intent; source at the relevant
revision establishes implementation.

## Content views and smart output

`minify`, `concise`, schema compaction, semantic chunking, and response pagination
serve different purposes. Treat them as separate controls.

| Control | Purpose | How to use the result |
|---|---|---|
| `minify:"none"` | Preserve selected source or supported exact history content, apart from security redaction. | Use for quotes, syntax, comments, and diff evidence. |
| `minify:"standard"` | Reduce content with a file- or surface-specific transformation. | Inspect the effective view; do not infer that removed text was absent in source. |
| `minify:"symbols"` | Extract a file outline where supported. | Use line anchors to read bodies; it is not a complete source view. |
| `concise` | Select a smaller discovery payload where the operation supports it. | Inspect the operation schema; it is not a universal minification flag. |
| `--compact` | Reduce CLI envelope and repeated metadata. | Resolve `base` and top-level `shared` values before interpreting rows. |
| Character window | Bound the selected or transformed content. | Follow returned continuations; offsets are not source-line numbers. |

File reads expose `none`, `standard`, and `symbols`. History is operation-specific:
PR detail exposes `none`/`standard`; discovery, issue detail, and
commit/compare operations do not accept a `minify` field. History has no
`symbols` mode. Do not send file-read modes to an operation that rejects them.

Read defaults, match preservation, fallback behavior, and window semantics in
the [tool reference](OCTOCODE_TOOLS.md). Do not infer local/remote equivalence
from equal field names. A minification extension entry is not evidence of a
structural grammar, outline extractor, graph resolver, or installed LSP server.

## Follow the complete continuation contract

Inspect every result row, including `status`, `meta.evidence`,
`meta.diagnostics`, and the tool's `data`. Public response shaping removes
free-form `warnings`; rely on typed diagnostics and effective content metadata.
Compact output can hoist shared values and shorten displayed paths. Use returned
executable queries rather than reconstructing paths from display strings.

For each partial surface, execute the relevant `next.*` call with its supplied
tool and query. Do not advance every counter together or calculate the next
offset from the requested character length. Semantic chunking can expand a
window to a boundary; page counters can be estimates while the continuation
offset is authoritative.

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

The acceptance contract requires a schema-valid executable continuation for
reachable partial data, or an explicit terminal-limit diagnostic when the bound
cannot be continued. That contract is a testing requirement, not proof that a
particular response or provider is complete. Repeating content, missing
continuations, or unreachable offsets are defects to reproduce and report.

## State what the evidence establishes

Record the claim, source path and revision, evidence type, traversed scope, and
remaining uncertainty. Source paths and line numbers support code claims; PR and
commit identities support history claims. A transformed view needs an exact read
before it supports a quote.

Stop when the question has sufficient evidence. Extend the investigation when a
coverage gap changes the decision. A provider cap, unavailable language server,
unresolved graph edge, or excluded file prevents a universal absence claim; it
does not require repeating the same unproductive query.
