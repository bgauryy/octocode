# Octocode tools reference

For a reader who needs each tool's **result behavior**: what a call returns, what a result proves, and the engine rules (language servers, Rust context, root selection, clone and graph handling) the schema does not carry. It does not repeat the other owners:

| Topic | Owner |
|---|---|
| Field names, defaults, ranges and field descriptions | the published schema: `octocode schema <tool> --view query` (generated from core) |
| Request and result envelope, rows, `next`, `related`, `hints`, paging | [Tool data contract](TOOL_DATA_CONTRACT.md) |
| Research routing, shape, recovery and measured cost | [Tool flow](../skills/octocode-research/references/tool-flow.md) |
| Availability, opt-ins, storage and caches | [Configuration](CONFIGURATION.md#tool-availability-cli-and-mcp) |
| `classification` and `clasify` | [Classification guide](OCTOCODE_CLASIFY.md) |

The catalog has 16 tools, all listed and enabled in the CLI; MCP exposes a configured subset. Listing a tool does not guarantee its provider or language server is ready; invocation reports those failures.

## How every tool call works

[Tool data contract](TOOL_DATA_CONTRACT.md) owns the envelope: `{"queries":[...],"responseChars":20000}` to start, `{"cursor":"…"}` alone to resume, indexed result rows, `next.*`, `related[]`, `hints[]` and the rule to execute emitted inputs unchanged. Discovery commands:

```bash
octocode schema
octocode schema localSearch --view query
octocode schema localSearch --view variants
```

`schema --view full` prints every tool with the envelope and rules. Core owns all schemas, descriptions, defaults and limits; use discovery for the exact field inventory instead of constructing private progress fields. The same concept has one canonical form in every tool:

| Concept | Canonical form |
|---|---|
| Text matching | `matchString:["term"]`, `regexEngine`, `caseMode`, `wholeWord` |
| Surrounding lines | `contextLines:{"before":2,"after":3}` |
| Inclusive line spans | `ranges:[{"startLine":1,"endLine":40}]` |
| Numeric bounds | `{min,max}`; omit an unbounded endpoint |
| Date bounds | `{from,to}`; omit an unbounded endpoint |
| Ordering | `sort:{by,order}` |
| Inline classification | `classification:{predicates:["Does this …?"],context?,longContext?}` |
| Output projection | `view`, with values advertised by that tool |

### Results, evidence, partial failures, and continuations

Check row status and partial-coverage facts even when the enclosing request succeeds: the exit code is not per row. See the [data contract](TOOL_DATA_CONTRACT.md#executable-continuations); a claim that rests on absence follows [tool flow](../skills/octocode-research/references/tool-flow.md). Pages use these literal keys:

| Key | Pages | Follow |
|---|---|---|
| `responsePagination{page,totalPages,hasMore,next}` | One response split across calls (`responseChars`) | `responsePagination.next` first |
| `rowPart{part,of}` | One row split across response pages | Read every part before using the row |
| `pagination{hasMore,…}` | A row's own list | The row's `next.<name>`, such as `next.nextPage` |
| `contentPagination.<section>` | One `ghGetHistoryItem` collection; `countScope` says what `totalItems` counts | That section's `next.*` |

## Internal, external, and hybrid tools

Local research tools read authorized files; `graphBuild` also writes derived snapshots under the workspace; GitHub and registry tools use their providers. `ghCloneRepo` materializes a repository for local analysis. `clasify` sends admitted evidence to the configured classification provider. Tool availability does not imply authentication or a running language server.

## Text, AST, graph, and LSP: choose the evidence you need

Three structural sources answer different questions; [tool flow](../skills/octocode-research/references/tool-flow.md) routes between them. Graph edges and AST matches are candidates: confirm callers or references with LSP before claiming code is unused.

| Source | Answers | Scope |
|---|---|---|
| Graph (`graphBuild`, `graphQuery`) | Whole-repository structure: imports, calls, cycles, change impact, ranked issues | Every file of one snapshot, from syntax; edges are candidates |
| LSP (`lspSearch`) | Exact per-symbol identity: definition, references, call hierarchy as the language server resolves them | One symbol at a time |
| AST (`astSearch`) | Per-pattern syntax: every place a code shape occurs | Matching source, no cross-file resolution |

AST rules can find calls inside methods, imports, inheritance clauses, and containment or ordering relationships. They return matching source, not a resolved graph of callers.

## GitHub tools reference

Fields: `octocode schema <tool> --view query`. Use a returned immutable commit identity for reproducible reads; GitHub's code index and provider result caps constrain search coverage, so follow every continuation the claim needs and keep terminal-limit disclosures.

### `ghSearchRepo`

Use a returned repository identity for code, tree and history calls.

```json
{"queries":[{"filters":{"language":"TypeScript"},"keywords":["code research"],"operation":"search"}]}
```

### `ghSearchCode`

Hits always come from the default-branch index: `ref` re-reads them at that revision and never widens the search. Read continuations reach evidence omitted from the displayed hit window.

```json
{"queries":[{"keywords":["localSearch"],"owner":"bgauryy","repo":"octocode"}]}
```

### `ghStructure`

```json
{"queries":[{"maxDepth":2,"operation":"tree","owner":"bgauryy","path":"packages","repo":"octocode"}]}
```

### `ghGetFileContent`

Copy emitted cursors to finish large reads. Local and GitHub file reads share the view behavior described under [`localFetch`](#localfetch).

```json
{"queries":[{"owner":"bgauryy","path":"src/index.ts","repo":"octocode"}]}
```

### `ghSearchHistory`

Search rows are an index, not a complete discussion or patch; read a selected item with `ghGetHistoryItem`.

```json
{"queries":[{"keywords":["schema"],"operation":"pullRequest","owner":"bgauryy","repo":"octocode"}]}
```

### `ghGetHistoryItem`

Files, comments, reviews, commits and patches can have independent continuations; complete each requested collection.

```json
{"queries":[{"number":1,"operation":"pullRequest","owner":"bgauryy","repo":"octocode"}]}
```

### `ghCloneRepo`

Execution requires persistent storage; MCP exposure has a separate [clone opt-in](CONFIGURATION.md#tool-availability-cli-and-mcp). A sparse checkout covers only its requested paths; retain that scope and the returned commit identity in local coverage claims. Checkouts live under `<OCTOCODE_HOME>/clones/`. The same commit, sparse selection and history depth replace a clean destination atomically; tracked edits, untracked or ignored files, local commits or Git refs (including branches, tags and stashes) prevent replacement. Clone has no cache lookup, TTL, eviction, cache metadata, or `refresh` field. Existing checkouts from older versions remain untouched. Storage and retained-work ownership: [configuration](STORAGE_AND_LIMITS.md#cache-storage-and-lifecycle).

```json
{"queries":[{"owner":"bgauryy","repo":"octocode"}]}
```

### `artifactSearch`

Provider-native package coordinates remain values, not hidden operation selectors. Results use `packageName` and may offer related source/release reads.

```json
{"queries":[{"ecosystem":"npm","operation":"resolve","packageName":"zod"}]}
```

## Local code tools reference

Fields: `octocode schema <tool> --view query`. Paths resolve against the workspace and must satisfy the read policy. Default exclusions and ignored paths are disclosed when relevant; a related broader search is a new scope, not proof that the first was complete.

### `localSearch`

Every shown match keeps its source coordinates. Count and scope disclosures describe what was actually searched.

```json
{"queries":[{"matchString":["registerHandler"],"path":"src"}]}
```

### `astSearch`

A syntax match does not prove a runtime call or reference. A scan cut by its time budget continues with `next.expandScan` after the files it evaluated; no file count or walk ceiling ends a scan.

```json
{"queries":[{"operation":"match","parser":"typescript","path":"src","rule":{"pattern":"eval($X)"}}]}
```

### `structureSearch`

`next.nextPage` continues through every page and internal scan window, so no walk ceiling ends a listing; `sort` orders each scan window before paging, so follow every continuation to finish the selected scope. A listing groups file entries by directory; read source separately. `view:"paths"` shows names only and reads no entry's metadata (unless a filter or sort needs it), so its `summary` has no `bytes` and a directory with its own group is named by that group alone. Entries without the sort key (a directory or link for `sizeBytes`; a directory, binary or unreadable file for `lineCount`) rank last in both orders.

```json
{"queries":[{"maxDepth":2,"operation":"tree","path":".","view":"paths"}]}
```

### `localFetch`

A symbols outline describes syntax, not resolved identity, and data outlines summarize containers rather than list every nested scalar. Inspect `viewFallback` when a symbols read cannot produce a useful outline. Markdown outlines recognize ATX (`#`) headings; underline-style Setext headings require content reads. A binary source produces a typed error. Secret scanning happens before projection, including range reads.

Compact reads preserve literal content and source-line citations. YAML compact reads retain the selected text, including comments and blank lines, because those bytes can belong to scalar values; `view:"symbols"` still outlines YAML keys. If a transformed view cannot be mapped to source lines, the reader keeps the original selected text. Use LSP to resolve a call's definition.

```json
{"queries":[{"path":"src/index.ts"}]}
```

## LSP tools reference

### `lspSearch`

Fields: `octocode schema lspSearch --view query`. A missing or still-indexing server does not prove absence. A row's `role` is `declaration`, `call`, `import` or `reference`. Follow importer, hierarchy and result continuations.

```json
{"queries":[{"line":10,"path":"src/index.ts","symbolName":"run"}]}
```

## Semantic assessment reference

### `clasify`

Evaluate supplied `content` or an unread `source`, discover models, or assess sections of keyed `resources`. Text sources and image assets keep evidence provenance without returning their provider payloads. `next.read` is the read needed to cite an answer's lines. Routes, limits and recovery: [classification guide](OCTOCODE_CLASIFY.md).

```json
{"queries":[{"content":"Create a replacement credential, then restart the service.","operation":"evaluate","questions":{"restart":{"instructions":"Does rotating the credential require restarting the service?","type":"noul"}}}]}
```

### Context selection

`classification` is supported by localSearch, localFetch, ghSearchCode and ghGetFileContent. The receipt distinguishes selected, related, excluded and unknown units; unknown is never a negative decision. Paging and absence rules (`next.nextCandidatePage`, `next.unfiltered`, `next.unfilteredUnknown`): [tool flow](../skills/octocode-research/references/tool-flow.md#scout-judge-what-you-have-not-read); the 64-unit page budget and the 0.5 keep threshold are in the schema.

### Rust build context

`rustContext` fields and limits are in the schema. A field left out keeps the effective value (the configured server options, else rust-analyzer's default); `features` are deduplicated and sorted for identity, and `target` is otherwise the server's Cargo environment.

- In a crate with a build script that did not run, an incoming answer missing files that spell the name is partial (`buildScriptsOff`) and offers a `related[]` lead with `rustContext.buildScripts:true`: the same query in that context. One note states the cost: it executes the workspace's build scripts (repository code) in a separate rust-analyzer (~2 GB).
- rust-analyzer loads only the package directory, so an `include!` of a file outside it never resolves: an answer missing files that spell the name through such an include is partial (`includeOutsidePackage`) with a warning naming the file; no `rustContext` recovers it; check those uses lexically with `localSearch`.
- A file that several files declare as a module (`#[path = "support/x.rs"] mod x;` in a bench and a test) is compiled once per declaring crate, and rust-analyzer answers for one copy: an incoming answer missing files that spell the name is partial (`sharedModuleFile`) with a warning naming the declaring files; check the other copies' uses lexically with `localSearch`.
- A context is an overlay: it changes only the fields it sets, and only where the value differs from the effective one. Everything else, `cfg(test)` code included, stays as the default server analyzes it, so `{}` or a context equal to the defaults reuses the default server. Build scripts and proc macros run only when `buildScripts`/`procMacros` is true; enabled providers run workspace code, and the context is not a sandbox.
- rust-analyzer supplies cfg-selected definitions, declarative macro expansion, and enabled build-script or proc-macro results; `astSearch` gets no compiler expansion. A disabled provider can explain an empty answer for a declaration a normal Cargo build generates.
- Different effective settings use different pooled clients. `rustContext.fingerprint` identifies the normalized requested settings and is part of pagination, so a changed context restarts pagination. It does not pin sources, Cargo configuration, toolchain, environment, or generated artifacts, and is not a reproducible-build identifier. See the [engine lifecycle contract](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md#rust-context) and [rust-analyzer configuration](https://rust-analyzer.github.io/book/configuration.html).

### Root selection

Without `workspaceRoot`, the root is the nearest ancestor of the file holding a project marker (`package.json`, `pnpm-workspace.yaml`, `yarn.lock`, `tsconfig.json`, `Cargo.toml`, `go.mod`, `pyproject.toml`, `requirements.txt`, `pom.xml`, `build.gradle`, `Makefile`, `.git`), even inside `WORKSPACE_ROOT`, widened for Rust to the Cargo workspace root, for TypeScript to the directory of the governing `tsconfig.json`/`jsconfig.json`, for Go to a `go.work` directory above, for Python to a `pyrightconfig.json` (or `pyproject.toml` with `[tool.pyright]` or `[tool.uv.workspace]`) above, and for Java to the Maven parent whose `<modules>` list the module or the Gradle settings that include it; with no marker anywhere, the file's own directory, never the process cwd. An explicit `workspaceRoot` must contain the file. A `workspaceRoot`-only query infers its language from the root's markers (`tsconfig.json`, `Cargo.toml`, `go.mod`, `pyproject.toml`, `setup.py`, `requirements.txt`, `go.work`, `compile_commands.json`, `.clangd`, `CMakeLists.txt`, `meson.build`, `pom.xml`, `build.gradle`, `build.gradle.kts`, `build.sbt`, `jsconfig.json`, `package.json`).

### Server fidelity and the no-fallback contract

Every `lspSearch` operation, including `documentSymbols`, needs a real server; without one the row is `serverUnavailable` with recovery through `localSearch` or `localFetch` ([semantic boundary](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md#semantic-boundary)). Markdown has no built-in server route: configure one, or use `localFetch view:"symbols"` for syntax-only heading outlines. There is no same-file-only `references` path. `diagnostic` on a server without pull diagnostics waits up to 1.5 s for a push after the document sync; a server that publishes only when its diagnostics change (typescript-language-server) keeps its last report, which is returned with a warning that it predates the last edit.

### Subtypes and implementations without an answer in time

`subtypes` on a server with no type hierarchy asks `textDocument/implementation`; `implementation` on a class, interface, struct or trait asks it directly. For both, a syntax scan of the files that spell the name (declarations whose `extends`/`implements` clause names it) runs at the same time. For `subtypes` it adds no wait; for `implementation`, a server answer also waits for the scan, and the declarations it found that the server left out (a function-local impl, say) follow as rows labelled `source: "native-graph-facts"`, partial (`partialReasons: ["scanOnlyImplementations"]`, no retry); page 1 states their count once (the `source` label's meaning is in the output schema). The server's rows win when they arrive: while the scan runs and for up to 20 s after it finishes. If the request fails (timeout, crash, rejected) or is still silent after that grace, the scan's rows are the answer, labelled `lsp.source: "native-graph-facts"`, partial (`partialReasons: ["serverUnanswered"]`) with a `related` rerun to ask the server again, and a note that they are unverified (the base is matched by name, not resolved to the declaration) and direct only (run the operation on each to descend). Files the scan could not read are named in a warning, add `heritageUnread` to `partialReasons`, and get a `related` text search over them. A server answer is labelled `lsp.source: "implementation"`; for `subtypes`, direct and transitive subtypes are not told apart. rust-analyzer answers a trait with its impl blocks: each trait impl (`impl Trait for Type`) is a direct subtype row with `kind: "impl"`, named by its self type, and has no subtypes of its own. A `subtypes` walk checks the anchor type's candidates 200 at a time: past the first window the row is partial (`hierarchyCandidateLimit`) and its last page offers `next.nextCandidatePage`; a deeper type with more than 200 candidates is checked 200 at a time too: the walk checks its first window, adds `hierarchyCandidateLimit`, and resumes it with `next.continueWalk*` as the anchor of its own walk, whose candidates page the same way. `implementation` on a function, method or other member has no scan: a timeout stays an error with the retry tip and the astSearch/localSearch route.

### TypeScript backends

1. An `lsp-servers.json` entry for the extension ([custom servers](#custom--bring-your-own-servers)); it always wins and gets no substitute.
2. `tsgo`, for TypeScript (`.ts`, `.mts`, `.cts`, `.tsx`) only: the workspace's own `node_modules/.bin/tsgo` when `lsp.trustProjectConfig` (`OCTOCODE_TRUST_PROJECT_LSP_CONFIG=true`) is on, else the `@typescript/native-preview` platform executable in Octocode's install tree (`octocode-mcp` and the `octocode` CLI depend on it). JavaScript stays on `typescript-language-server`, because tsgo drops constructor functions and `this` expandos. tsgo returns at most 256 workspace symbols; a `workspaceSymbol` answer that reaches 256 warns that the server capped it (narrow `symbolQuery`, or route `.ts` to `typescript-language-server` in `lsp-servers.json`).
3. `typescript-language-server`, for JavaScript, and for TypeScript when no `tsgo` is found or it fails to start (the answer says so): an executable on `PATH`; else `node_modules/typescript-language-server/lib/cli.mjs` through the current Node, from Octocode's install tree, then the workspace's `node_modules` only when the workspace is trusted, then the start directory. Trusted here means `lsp.trustProjectConfig` or a workspace at or inside the start directory, so a scanned checkout such as a clone cannot supply the executable.

Definition locations stay server output; import targets are never rewritten. LSP reads minified `.js`, but original source gives far better results.

### Language servers

The built-in server per language is the one table in [Supported languages and features](../packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md); TypeScript routing is in [TypeScript backends](#typescript-backends). Assembly has no built-in server. Rust and C/C++ support managed downloads (`octocode lsp-server install <name>`); others use installed executables, and a missing one's error names its install command. Headless start options, the memory cap and its watchdog, pooling and tracing: [LSP server lifecycle](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md).

Lines end at `\r\n`, `\n`, or a lone `\r` (the LSP rule). A location in an allowed file that cannot be read (too large, not UTF-8) is kept, with `content` stating why.

#### Custom / bring-your-own servers

An `lsp-servers.json` entry (format, load order and trust: [lifecycle](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md#custom-configuration), [configuration](CONFIGURATION.md)) adds an extension or replaces a built-in server. An entry for one TypeScript or JavaScript spelling (`.ts`) routes the whole family (`.mts`, `.cts`, `.tsx`), each file keeping its built-in language id, so one project is not split across two servers. The server answers what it advertises; an extension with no entry and no built-in route returns `serverUnavailable`.

<!-- example: configuration -->
```jsonc
{"languageServers": {".php": {"command": "intelephense", "args": ["--stdio"], "languageId": "php"}}}
```

## Clone and local tools workflow

Clone a verified repository/ref, then use the returned path with localSearch, structureSearch, astSearch or lspSearch. Keep the materialization scope in the claim.

### `graphBuild` and `graphQuery`

Build or reuse a syntax graph with `graphBuild`, then explore the exact scope with `graphQuery`; use `--json` for CLI machine output. They are the only graph surface: the CLI has no separate graph command. Both are always available as CLI commands; [configuration](CONFIGURATION.md#tool-availability-cli-and-mcp) controls MCP exposure. `graphBuild` writes snapshots and prunes older retained ones; `graphQuery` is read-only. Query continuations pin the snapshot, and every successful query reports freshness.

Call traversal defaults to high confidence. Verify consequential findings with LSP and source reads: `impact` rows reached through imports are module-level candidates, so confirm a symbol change breaks them with `lspSearch` references. For bulk evidence, send `responseChars: 50000`, list up to the schema's maximum of sources as `witnesses`/`unresolved`/`symbols` `target`, and filter `unresolved` with `reasons` before paging. Scope, snapshot and coverage rules: [tool flow](../skills/octocode-research/references/tool-flow.md#graph); architecture: [code graph](../packages/octocode-native/docs/engine/CODE_GRAPH.md#agent-graph-tools-cli-and-beta-mcp). `includeProvenance` is defined once in [Minimal by default](TOOL_DATA_CONTRACT.md#minimal-by-default).
