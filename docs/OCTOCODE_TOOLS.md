# Octocode tools reference

The catalog has 17 tools. Agents call them over [MCP](OCTOCODE_MCP.md), or run them in the [CLI](OCTOCODE_CLI.md) with `octocode <tool> '<JSON input>'` (add `--json` for structured output). Which tools are enabled depends on [configuration](CONFIGURATION.md#tool-availability-cli-and-mcp); an MCP client lists its enabled tools with `tools/list`.

| Start with | What the agent can find |
|---|---|
| [A local checkout](#local-code-tools-reference) | Files, exact lines, declarations, callers and repository-wide dependencies |
| [A GitHub repository](#github-tools-reference) | Files at a revision, commits, pull requests, issues and patches |
| [A package name or lockfile](#package-and-advisory-tools) | The published version, its source revision and known advisories |
| [Unread evidence](#semantic-assessment-reference) | Files or sections worth opening, using a configured classification provider |

## How a call works

Send `{"queries":[…]}`, up to 5 queries per call. Fields and defaults: `octocode schema <tool> --view query`. Read every row, then follow its continuations: [executable continuations](TOOL_DATA_CONTRACT.md#executable-continuations).

<a id="text-ast-graph-and-lsp-choose-the-evidence-you-need"></a>

## Choose the evidence you need

| Question | Start with | What the result establishes |
|---|---|---|
| Where does this text occur? | `localSearch` | Matches within the searched scope |
| Which calls have this shape? | `astSearch` | Parsed syntax at each source location, not identity |
| Which files might this change affect? | `graphBuild` → `graphQuery` | Candidates from a snapshot's import and call edges |
| Which symbol does this name resolve to? | `lspSearch` | The editor or language server's answer, when one is available. Check `lsp.level`. |

## Local code tools reference

Chain for a call site: `localSearch` finds the text, `lspSearch` resolves the symbol, `localFetch` reads the call sites. `structureSearch` locates files first; `astSearch` finds a call shape.

### `structureSearch`

Lists files and directories, finds a half-known path, or ranks files by size, lines or date.

- `operation:"tree"` lists a directory. `operation:"files"` is needed for sorts and filters on size, date or name.
- `view:"paths"` gives names. `view:"details"` adds bytes, lines and modified time, and reads each file to count lines.
- Check skipped paths when the answer depends on coverage.

```json
{"queries":[{"operation":"files","path":".","entryType":"file","sort":{"by":"bytes","order":"desc"},"limit":1}]}
```

### `localSearch`

Finds text, lists the files that have or lack it, or counts occurrences.

- Only `view:"content"` returns source. Read a long match's original line with `localFetch`.
- Nothing is ranked. To find where a name is defined, use `lspSearch` or `astSearch`.
- The default traversal returns pages in file-name order. Until the last page, `pagination.total` is a floor, and `matchCount`, `matchedLineCount` and `matchedFiles` are absent. For exact totals use `view:"countMatches"`, `view:"countLines"` or `sort:{"by":"matchCount","order":"desc"}`. These scan the whole scope before page 1.

```json
{"queries":[{"path":"packages/octocode/src","matchString":["runCLI"]}]}
```

### `localFetch`

Reads a known file.

- `ranges` takes up to 100 inclusive, 1-based ranges; past the end they clip with a warning. Without `ranges` or `matchString`, the whole file is read, paged.
- `matchString` with `block:true` reads the enclosing declaration. For a signature only, use `contextLines`.
- `viewFallback` names the view that was served when the requested one could not be.

| `view` | Use |
|---|---|
| `content` | Source after redaction. Quote it. |
| `symbols` | Outline. Doc comments sit above each entry, so quote from the line above. |
| `compact` | Comments and extra whitespace removed. Read it; do not quote it. |

### `astSearch`

Matches code shape, not spelling: calls, imports, inheritance, declarations, and a file's syntax tree.

| `operation` | Use |
|---|---|
| `match` | Nodes that fit a `rule` |
| `syntaxTree` | Node kinds of one file, to write a rule |
| `symbols` | Declarations by name and kind; `symbolFilter` narrows them |

- Rules use metavariables: `$X` matches one node, `$$$ARGS` zero or more.
- Supported grammars: TypeScript, TSX, JavaScript, Python, Go, Rust, Java, C, Assembly, Scala, C++, CUDA and C#. A parser does not imply a language server or cross-file linking. AST matches are candidates until `lspSearch` or the source confirms them.

```json
{"queries":[{"operation":"match","path":"packages/octocode/src","rule":{"pattern":"runCLI()"}}]}
```

### `graphBuild` and `graphQuery`

`graphBuild` builds a syntax graph of a path; `graphQuery` answers structural questions from it. The questions, options and how to read answers are in the [code graph guide](OCTOCODE_GRAPH.md).

- Use the same `path` for build and query. Check `stale` before you trust edges. Verify symbol-level impact or unused-code candidates with `lspSearch`.
- Snapshots go to `.octocode/graph` in the workspace. `local.graphHome` moves them, including for a clone from `ghCloneRepo`. [Graph settings](CONFIGURATION.md#all-settings-and-defaults)
- A long build pauses and returns `next.expandScan`. Run it to resume.

### `lspSearch`

Resolves definitions, references, callers, callees and type hierarchies at a symbol.

```json
{"queries":[{"operation":"references","path":"packages/octocode/src/cli/index.ts","symbolName":"runCLI","line":7,"view":"files"}]}
```

- Check `lsp.level` (below) before you use a result to establish identity. When present, `lsp.fellBackFrom` names the source that failed.
- Read `partialReasons` and any continuations before you claim every reference. Octocode infers the project root from the file; set `workspaceRoot` if it misses neighboring code. If workspace-symbol results hit a server's cap, narrow `symbolFuzzy`.

| `lsp.level` | What it means |
|---|---|
| `ide` | The editor's language features answered, through the VS Code extension's bridge. |
| `server` | A language server answered. A successful server result can omit the `lsp` receipt. |
| `syntax` | A grammar produced candidates by name or shape. Diagnostics at this level report parse errors. |
| `text` | No structural answer. Use the text-search or source-read leads. |

#### Subtypes and implementations without an answer in time

Rows labeled `native-graph-facts` are candidates from a scan of inheritance declarations; check them in source. Follow any hierarchy continuation to reach the requested depth.

#### Language servers

| Need | Use |
|---|---|
| See which servers Octocode finds | `octocode lsp-server status` |
| Managed install | `rust-analyzer` and `clangd` ([CLI reference](OCTOCODE_CLI.md)) |
| Add or replace a server | `lsp-servers.json`. An explicit entry takes priority. |

#### TypeScript backends

For TypeScript, Octocode prefers `tsgo` from a trusted project's install or the bundled copy, then falls back to `typescript-language-server`. JavaScript uses `typescript-language-server`.

**Rust build scripts and proc macros** run only when `lsp.allowBuildScripts` is set and the query's `rustContext` asks for them. [Settings](CONFIGURATION.md#all-settings-and-defaults)

#### Fetched code

No language server starts on clones, materialized GitHub files or URL captures. Text and AST tools still read them. To allow it, set `lsp.trustFetchedCode` in home settings or the process environment. [Trust for project code](SECURITY.md#trust-for-project-code)

## GitHub tools reference

Chain for a change: `ghSearchHistory` finds the commit, `ghGetHistoryItem` reads its patch, and `ghGetFileContent` reads the file at that commit. Example: in `octocat/Hello-World`, the history of `README` includes a commit that adds a final newline, and its patch shows the changed line.

### `ghSearchRepo`

Finds repositories by keyword or filter, lists an owner's repositories, or gets one.

- Archived repositories are excluded unless `filters.archive` says otherwise.
- Carry the returned `owner` and `repo` into later calls.

### `ghSearchCode`

Finds a term, path or file name in GitHub's code index.

- `ref` re-reads hit lines at that revision, but which files match is still decided on the default branch. For a file that exists only on another branch, start with `ghStructure`.
- The index may not cover every file. An empty search does not prove absence.

### `ghStructure`

Browses a tree, lists refs, or reports languages.

- Use `ref` for another branch, tag or commit. The result names the commit; keep it with your findings.
- `materialize:true` downloads the selected files for local tools. `location` reports coverage and skipped files. Use it when a few paths need many searches. [Materialized store](STORAGE_AND_LIMITS.md#materialize-git-store)

### `ghGetFileContent`

Reads part of a file at a known path. Pass `ref` as the commit SHA from earlier results to cite a fixed revision. Views match [`localFetch`](#localfetch).

### `ghSearchHistory`

Finds pull requests, issues or commits, for why, when or who questions.

- A `commit` query with `path` and no keywords lists that path's changes. Open the returned commit SHA or PR/issue number next.

```json
{"queries":[{"operation":"commit","owner":"octocat","repo":"Hello-World","path":"README"}]}
```

### `ghGetHistoryItem`

Reads one pull request, issue or commit, or compares two refs.

- Omit `sections` for the summary. Up to 8 sections per call.
- One file's diff: `sections:["patches"]` with `include` (a root-anchored glob).
- A paged section appears in `sectionPagination`, with a matching `next.<section>` call.

### `ghCloneRepo`

Creates a local checkout at a revision, for repeated analysis with the local tools.

- `paths` makes a sparse checkout (up to 100 subtrees); completeness then covers only those paths.
- Use `location.path` and keep the commit and path scope with your findings. An existing checkout with local changes is kept.
- Needs Git and persistent storage. [Clone opt-in](CONFIGURATION.md#tool-availability-cli-and-mcp) · [Storage](STORAGE_AND_LIMITS.md#cache-storage-and-lifecycle). No language server starts on the clone; see [fetched code](#fetched-code).

## Package and advisory tools

Use the locked version from the lockfile for both lookups. Source: `artifactSearch`, then `ghStructure` and `ghGetFileContent` at the verified ref. Advisories: `securityCheck`.

### `artifactSearch`

Resolves a package version to its source, or searches package names.

- Ecosystems: `npm`, `pypi`, `crates`, `maven`, `nuget`, `go`, `packagist`, `rubygems`. Keyword `search` works for all except `pypi`.
- Without `version` (or with `latest`), the newest stable release is used; npm uses its `latest` dist-tag. That may not be your project's version, so pass the locked one.
- `view:"details"` adds dependencies and license. Only `pypi` and `nuget` list advisories here; an empty list elsewhere says nothing.
- `verification`: `provenance` and `tag` are checked; `registryRef` is an unchecked registry claim. `registryUrl` applies only to a private npm registry.
- To compare two versions, use `ghGetHistoryItem` with `operation:"compare"`.

```json
{"queries":[{"operation":"resolve","ecosystem":"npm","packageName":"zod","version":"3.22.4"}]}
```

### `securityCheck`

Checks known advisories and published fixes for a package version, purl, URL, advisory ID, commit or GitHub repository, using OSV and GitHub.

- The summary gives `affected`, `upgradeTo` and `advisoryCount` when established. `advisoryCount` 0 means no known advisory, not proof of safety. `error` means not checked. `view:"details"` gives records, sources and affected ranges.
- Without `version`, the latest stable release is checked where the registry is read; otherwise every version is.
- **Availability:** the CLI offers it by default. Over MCP it is off until you set `tools.enableSecurityCheck` (or `OCTOCODE_SECURITY_CHECKS=true`) and restart. [Settings](CONFIGURATION.md#all-settings-and-defaults)
- What each operation sends: [Advisory egress](SECURITY.md#advisory-egress).

## Semantic assessment reference

### `clasify`

Judges content that exact search cannot split, such as long documents, logs or many files, using a classification key and a configured provider.

- Fresh evaluations send evidence to the provider and cost usage.
- A row's `next.read` leads to the supporting lines. Cite those lines, not the classification.
- Before you claim absence after a classification, check `next.nextCandidatePage` (billed), `next.unfilteredUnknown` and `next.unfiltered`.
- [Setup, examples and costs](OCTOCODE_CLASIFY.md)

### Context selection

`classification` is supported by `localSearch`, `localFetch`, `ghSearchCode`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem` and `structureSearch`. Use a predicate to filter hits, source chunks, paths or discussion rows before you read them. Check the receipt for excluded and unknown units. [Classification guide](OCTOCODE_CLASIFY.md)
