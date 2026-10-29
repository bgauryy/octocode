# Octocode tools reference

The full catalog has 16 tools. The default catalog contains 12 tools over MCP and 15 in the CLI. `ghCloneRepo` is CLI-only; `OCTOCODE_BETA=true` adds `graphBuild` and `graphQuery` to MCP while CLI graph tools remain available; `clasify` requires a classification credential and a successful provider ping. Live discovery reports availability for the current configuration.

## How every tool call works

### Base call envelope

Start work with `{"queries":[query,...],"responseChars":20000}`. `responseChars` is optional. Batch up to five independent queries of the same tool; `graphBuild` accepts one. Resume saved work with only `{"cursor":"…"}`, copied from an emitted call. CLI and MCP use the same envelope; tool names and fields are case-sensitive.

A query selects an explicit operation or view when required by that tool's schema. Unknown fields, cross-operation fields and malformed values fail validation. The runtime does not guess intent, coerce aliases or rewrite the query.

### Schema discovery, variants, defaults, and hints

```bash
octocode schema
octocode schema localSearch --view query
octocode schema localSearch --view variants
```

The query view describes authored fields. The variants view lists each operation shape with its required fields and one example. The catalog form `octocode schema --view full` prints every tool with envelope and rules. Core owns all schemas, descriptions, defaults and limits; use discovery for the exact field inventory instead of constructing private progress fields.

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

Each result has its input `index` and `data`. Check row status and partial-coverage facts even when the enclosing request succeeds. `next.*` and `responsePagination.next` contain `{kind:"continue",tool,input:{cursor}}`; `related[]` contains `{kind:"related",tool,input:{queries:[...]}}`. Execute the supplied `input` unchanged. `hints[]` contains prose only. See the [data contract](TOOL_DATA_CONTRACT.md).

## Internal, external, and hybrid tools

Local research tools read authorized files; `graphBuild` also writes derived snapshots under the workspace; GitHub and registry tools use their providers. `ghCloneRepo` materializes a repository for local analysis. `clasify` sends admitted evidence to the configured classification provider. Tool availability does not imply authentication or a running language server.

## Text, AST, graph, and LSP: choose the evidence you need

Use text search for exact spelling, AST for syntax, LSP for semantic locations, and history for changes. Graph edges and AST matches are candidates; confirm callers or references with LSP before claiming code is unused. [Research flows](OCTOCODE_WORKFLOWS.md) connect these tools.

## GitHub tools reference

### Shared GitHub rules

Use separate `owner` and `repo` names. A `ref` selects a revision where the tool supports it; use a returned immutable commit identity for reproducible reads. GitHub's code index and provider result caps constrain search coverage. Follow every continuation needed for the claim, and retain terminal-limit disclosures.

### `ghSearchRepo`

Find repositories with `operation:"search"`, list an owner with `"list"`, or inspect one repository with `"get"`. Search filters belong in the typed `filters` object. Use a returned repository identity for code, tree and history calls.

```json
{"queries":[{"filters":{"language":"TypeScript"},"keywords":["code research"],"operation":"search"}]}
```

### `ghSearchCode`

Search GitHub’s code index with `keywords`. Scope with owner/repository and typed filters. The index may lag or omit a requested revision; read returned paths at the intended ref before making a source claim. Read continuations reach evidence omitted from the displayed hit window.

```json
{"queries":[{"keywords":["localSearch"],"owner":"bgauryy","repo":"octocode"}]}
```

### `ghStructure`

Inspect a repository tree, refs or language inventory using `operation:"tree"`, `"refs"` or `"languages"`. Tree depth bounds the requested view. `include` and `exclude` are glob arrays; do not embed an implicit pattern language in a path.

```json
{"queries":[{"maxDepth":2,"operation":"tree","owner":"bgauryy","path":"packages","repo":"octocode"}]}
```

### `ghGetFileContent`

Read a known GitHub path. A path-only request reads source content; `ranges` selects inclusive spans and `matchString` selects matched regions. `view:"compact"` transforms content; `view:"symbols"` outlines declarations. Match selection and view combinations are explicit schema variants. Copy emitted cursors to finish large reads.

```json
{"queries":[{"owner":"bgauryy","path":"src/index.ts","repo":"octocode"}]}
```

### `ghSearchHistory`

Find pull requests, issues or commits with `operation`. `keywords` searches text, `filters` holds typed provider filters, and `sort` has `by` and `order`. Use `pathPrefix` for commit path scope. Read a selected item with `ghGetHistoryItem`; search rows are an index, not a complete discussion or patch.

```json
{"queries":[{"keywords":["schema"],"operation":"pullRequest","owner":"bgauryy","repo":"octocode"}]}
```

### `ghGetHistoryItem`

Read a pull request or issue by `number`, a commit by `ref`, or a comparison by `baseRef` and `headRef`. `sections` selects the evidence needed. Files, comments, reviews, commits and patches can have independent continuations; complete each requested collection. Matcher fields have the same meaning as in other text tools.

```json
{"queries":[{"number":1,"operation":"pullRequest","owner":"bgauryy","repo":"octocode"}]}
```

### `ghCloneRepo`

Materialize a cached checkout for local tools. Requires persistent storage and is exposed through the CLI. Use the returned local path for search, AST and LSP. A sparse checkout has a scoped view; follow materialization continuations before claiming complete local coverage.

```json
{"queries":[{"owner":"bgauryy","repo":"octocode"}]}
```

### `artifactSearch`

Resolve a named package with `operation:"resolve"`, `ecosystem` and `packageName`; use `version` separately when supported. Discover packages with `operation:"search"` and `keywords`. `registryUrl` selects a registry. Provider-native package coordinates remain values, not hidden operation selectors. Results use `packageName` and may offer related source/release reads.

```json
{"queries":[{"ecosystem":"npm","operation":"resolve","packageName":"zod"}]}
```

## Local code tools reference

### Shared local rules

Paths resolve against the workspace and must satisfy the read policy. Search tools share scope filters. Default exclusions and ignored paths are disclosed when relevant; a related broader search is a new scope, not proof that the first was complete. Returned paths are workspace-relative when possible.

### `localSearch`

Search authorized local text with `matchString` as an array. Choose the advertised regex engine and case mode explicitly when defaults are unsuitable. Views include content, files, filesWithout, countLines, countMatches and matchOnly. `unique` applies to matchOnly; `multiline` is off, on or dotall. Every shown match keeps its source coordinates and complete enclosing declaration when available. Count and scope disclosures describe what was actually searched.

```json
{"queries":[{"matchString":["registerHandler"],"path":"src"}]}
```

### `astSearch`

Use `operation:"match"` with a typed `rule` object, `"symbols"` for declarations, or `"syntaxTree"` for parse structure. `parser` chooses a parser when required. `symbolFilter` is a typed mode/terms filter. A syntax match does not prove a runtime call or reference. Declaration nodes own their evidence fields; each result path resolves against the workspace or is an admitted absolute path.

```json
{"queries":[{"operation":"match","parser":"typescript","path":"src","rule":{"pattern":"eval($X)"}}]}
```

### `structureSearch`

Use `operation:"tree"` to inspect a directory scope or `"files"` to discover paths. Shared include/exclude globs and explicit metadata filters control the walk. `entryType` uses descriptive enum values such as `file` and `directory`. A listing groups file entries by directory; read source separately. `view:"paths"` requests names with directory and symlink markers, omitting size, time and line-count details. The default `view:"details"` retains those details. Sorting by `lineCount` still reads file content in either view.

```json
{"queries":[{"maxDepth":2,"operation":"tree","path":".","view":"paths"}]}
```

### `localFetch`

Read a known local file. Path-only reads exact content; `ranges` uses inclusive startLine/endLine objects, and `matchString` selects matched regions. `contextLines` sets before/after context. `view:"compact"` transforms content; `view:"symbols"` outlines it. A binary source produces a typed error. Secret scanning happens before projection, including range reads.

```json
{"queries":[{"path":"src/index.ts"}]}
```

## LSP tools reference

### `lspSearch`

Use observed `path`, `line` and `symbolName` anchors for definitions, references, callers, callees, hover and type navigation. `occurrence` distinguishes repeated names on one line (zero-based). `workspaceSymbol` takes `symbolQuery` and a path or workspaceRoot; document operations take a path. Location output uses `view:"content"` or `"files"`. Follow importer, hierarchy and result continuations. Missing or indexing servers do not prove absence.

```json
{"queries":[{"line":10,"path":"src/index.ts","symbolName":"run"}]}
```

## Semantic assessment reference

### `clasify`

Evaluate supplied `content` or an unread `source`, discover models, or assess sections of keyed `resources`. Questions request typed answers. Text sources and image assets keep evidence provenance without returning their provider payloads. Use inline `classification` on search/read when selecting retrieved evidence is the task. See the [classification guide](OCTOCODE_CLASIFY.md).

```json
{"queries":[{"content":"Create a replacement credential, then restart the service.","operation":"evaluate","questions":{"restart":{"instructions":"Does rotating the credential require restarting the service?","type":"noul"}}}]}
```

### Context selection

`classification` is supported by localSearch, localFetch, ghSearchCode and ghGetFileContent. The receipt distinguishes selected, related, excluded and unknown units. Complete selected-view continuations for a selected claim; follow unfiltered or inspection continuations when the claim needs omitted evidence. Unknown is never a negative decision.

### Rust build context

| `rustContext` field | Default in an explicit context | Meaning |
|---|---|---|
| `features` | `[]` | Cargo features, or `"all"`; deduplicated and sorted for identity. |
| `noDefaultFeatures` | `false` | Disable default features. |
| `target` | unset (the server's Cargo environment) | Target triple, 1–256 chars. |
| `cfgs` | `[]` | ≤100 cfg settings added to the effective list (`"custom"`, `"mode=fast"`); `"!custom"` disables one. |
| `buildScripts` | `false` | Run build scripts and load their cfg and generated sources. |
| `procMacros` | `false` | Expand procedural macros; requires `buildScripts: true`. |

- In a crate with a build script that did not run, an incoming answer missing files that spell the name is partial (`buildScriptsOff`) and offers `includeBuildScripts` in `related`: the same query with `rustContext.buildScripts: true`. One note states the cost: it executes the workspace's build scripts (repository code) in a separate rust-analyzer (~2 GB).
- rust-analyzer loads only the package directory, so an `include!` of a file outside it never resolves: an answer missing files that spell the name through such an include is partial (`includeOutsidePackage`) with a warning naming the file; no `rustContext` recovers it; check those uses lexically with `localSearch`.
- A file that several files declare as a module (`#[path = "support/x.rs"] mod x;` in a bench and a test) is compiled once per declaring crate, and rust-analyzer answers for one copy: an incoming answer missing files that spell the name is partial (`sharedModuleFile`) with a warning naming the declaring files; check the other copies' uses lexically with `localSearch`.
- A context is an overlay: it changes only the fields it sets, and only where the value differs from the effective one (the configured server options, else rust-analyzer's default). Everything else, `cfg(test)` code included, stays as the default server analyzes it, so `{}` or a context equal to the defaults reuses the default server. Build scripts and proc macros run only when `buildScripts`/`procMacros` is true; enabled providers run workspace code, and the context is not a sandbox.
- rust-analyzer supplies cfg-selected definitions, declarative macro expansion, and enabled build-script or proc-macro results; `astSearch` gets no compiler expansion. A disabled provider can explain an empty answer for a declaration a normal Cargo build generates.
- Different effective settings use different pooled clients. `rustContext.fingerprint` identifies the normalized requested settings and is part of pagination, so a changed context restarts pagination. It does not pin sources, Cargo configuration, toolchain, environment, or generated artifacts, and is not a reproducible-build identifier. See the [engine lifecycle contract](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md#rust-context) and [rust-analyzer configuration](https://rust-analyzer.github.io/book/configuration.html).

### Root selection

Without `workspaceRoot`, the root is the nearest ancestor of the file holding a project marker (`package.json`, `pnpm-workspace.yaml`, `yarn.lock`, `tsconfig.json`, `Cargo.toml`, `go.mod`, `pyproject.toml`, `requirements.txt`, `pom.xml`, `build.gradle`, `Makefile`, `.git`), even inside `WORKSPACE_ROOT`, widened for Rust to the Cargo workspace root, for TypeScript to the directory of the governing `tsconfig.json`/`jsconfig.json`, for Go to a `go.work` directory above, for Python to a `pyrightconfig.json` (or `pyproject.toml` with `[tool.pyright]` or `[tool.uv.workspace]`) above, and for Java to the Maven parent whose `<modules>` list the module or the Gradle settings that include it; with no marker anywhere, the file's own directory, never the process cwd. An explicit `workspaceRoot` must contain the file. A `workspaceRoot`-only query infers its language from the root's markers (`tsconfig.json`, `Cargo.toml`, `go.mod`, `pyproject.toml`, `setup.py`, `requirements.txt`, `go.work`, `compile_commands.json`, `.clangd`, `CMakeLists.txt`, `meson.build`, `pom.xml`, `build.gradle`, `build.gradle.kts`, `build.sbt`, `jsconfig.json`, `package.json`).

### Server fidelity and the no-fallback contract

`documentSymbols` falls back to a native outline (`lsp.source`: `native` via oxc for JS/TS, or `markdown` headings; syntax-only) only when no server is available; a server (`lsp`: type-aware, cross-file) always wins. Every other operation needs a real server. Without one, Octocode does not guess: `status:"error"`, `errorCode:"serverUnavailable"`, and a message that points to `localSearch` or `astSearch` + `localFetch`. There is no same-file-only `references` path. `diagnostic` on a server without pull diagnostics waits up to 1.5 s for a push after the document sync; a server that publishes only when its diagnostics change (typescript-language-server) keeps its last report, which is returned with a warning that it predates the last edit. See [LSP server lifecycle](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md).

### Subtypes and implementations without an answer in time

`subtypes` on a server with no type hierarchy asks `textDocument/implementation`; `implementation` on a class, interface, struct or trait asks it directly. For both, a syntax scan of the files that spell the name (declarations whose `extends`/`implements` clause names it) runs at the same time. For `subtypes` it adds no wait; for `implementation`, a server answer also waits for the scan, and the declarations it found that the server left out (a function-local impl, say) follow as rows labelled `source: "native-graph-facts"`, partial (`partialReasons: ["scanOnlyImplementations"]`, no retry); page 1 states their count once (the `source` label's meaning is in the output schema). The server's rows win when they arrive: while the scan runs and for up to 20 s after it finishes. If the request fails (timeout, crash, rejected) or is still silent after that grace, the scan's rows are the answer, labelled `lsp.source: "native-graph-facts"`, partial (`partialReasons: ["serverUnanswered"]`) with a `related` rerun to ask the server again, and a note that they are unverified (the base is matched by name, not resolved to the declaration) and direct only (run the operation on each to descend). Files the scan could not read are named in a warning, add `heritageUnread` to `partialReasons`, and get a `related` text search over them. A server answer is labelled `lsp.source: "implementation"`; for `subtypes`, direct and transitive subtypes are not told apart. rust-analyzer answers a trait with its impl blocks: each trait impl (`impl Trait for Type`) is a direct subtype row with `kind: "impl"`, named by its self type, and has no subtypes of its own. A `subtypes` walk checks the anchor type's candidates 200 at a time: past the first window the row is partial (`hierarchyCandidateLimit`) and its last page offers `next.nextCandidatePage`; a deeper type with more than 200 candidates stays a terminal limit. `implementation` on a function, method or other member has no scan: a timeout stays an error with the retry tip and the astSearch/localSearch route.

### TypeScript backends

1. An `lsp-servers.json` entry for the extension ([custom servers](#custom--bring-your-own-servers)); it always wins and gets no substitute.
2. `tsgo`, for TypeScript (`.ts`, `.mts`, `.cts`, `.tsx`) only: the workspace's own `node_modules/.bin/tsgo` when `lsp.trustProjectConfig` (`OCTOCODE_TRUST_PROJECT_LSP_CONFIG=true`) is on, else the `@typescript/native-preview` platform executable in Octocode's install tree (`octocode-mcp` and the `octocode` CLI depend on it). JavaScript stays on `typescript-language-server`, because tsgo drops constructor functions and `this` expandos. tsgo returns at most 256 workspace symbols; a `workspaceSymbol` answer that reaches 256 warns that the server capped it (narrow `symbolQuery`, or route `.ts` to `typescript-language-server` in `lsp-servers.json`).
3. `typescript-language-server`, for JavaScript, and for TypeScript when no `tsgo` is found or it fails to start (the answer says so): an executable on `PATH`; else `node_modules/typescript-language-server/lib/cli.mjs` through the current Node, from Octocode's install tree, then the workspace's `node_modules` only when the workspace is trusted, then the start directory. Trusted here means `lsp.trustProjectConfig` or a workspace at or inside the start directory, so a scanned checkout such as a clone cannot supply the executable.

Definition locations stay server output; import targets are never rewritten. LSP reads minified `.js`, but original source gives far better results.

### Language servers

The built-in server per language is the one table in [Supported languages and features](../packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md); TypeScript routing is in [TypeScript backends](#typescript-backends). Assembly has no built-in server. Rust and C/C++ support managed downloads (`octocode lsp-server install <name>`); others use installed executables, and a missing one's error names its install command.

Built-in servers start headless and read-only (user arguments and options win): rust-analyzer runs no build scripts, proc macros, or `cargo check`; clangd runs `--background-index=false --clang-tidy=false --log=error --pch-storage=memory`; jdtls keeps `-data` under the Octocode home with `java.autobuild.enabled:false`; Metals starts with `isHttpEnabled:false`. Each server tree is capped at 25% of host RAM, between 4096 and 8192 MiB (a per-server `maxMemoryMb` in lsp-servers.json overrides it; `0` disables); on macOS and Linux an RSS watchdog enforces it on the whole server tree ("language server exceeded memory cap").

Lines end at `\r\n`, `\n`, or a lone `\r` (the LSP rule). A location in an allowed file that cannot be read (too large, not UTF-8) is kept, with `content` stating why.

#### Custom / bring-your-own servers

An `lsp-servers.json` entry adds an extension or replaces a built-in server (another `rust-analyzer`, `tsgo`, a pinned `clangd`); an entry for one TypeScript or JavaScript spelling (`.ts`) routes the whole family (`.mts`, `.cts`, `.tsx`), each file keeping its built-in language id, so one project is not split across two servers; a relative command with a path separator resolves against the workspace root when the process directory has none. Load order: `lsp.configPath` / `$OCTOCODE_LSP_CONFIG`; `<workspace>/.octocode/lsp-servers.json` (only with `lsp.trustProjectConfig`); `~/.octocode/lsp-servers.json`.

<!-- example: configuration -->
```jsonc
{"languageServers": {".php": {"command": "intelephense", "args": ["--stdio"], "languageId": "php"}}}
```

`command` and `languageId` are required; `args` (default `[]`) and `initializationOptions` (sent verbatim in `initialize`) are optional. The server answers what it advertises; an extension with no entry and no built-in route returns `serverUnavailable`. See [`LSP_SERVER_LIFECYCLE.md`](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md#custom-configuration).


## Clone and local tools workflow

Clone a verified repository/ref, then use the returned path with localSearch, structureSearch, astSearch or lspSearch. Keep the materialization scope in the claim.

### Cache behavior

Storage and cache ownership are documented in [configuration](CONFIGURATION.md#cache-storage-and-lifecycle). A cursor belongs to its issuing tool, access context and saved state. Expiry requires an explicit fresh request.

### `graphBuild` and `graphQuery`

Build or reuse a syntax graph with `graphBuild`, then explore the exact scope with
`graphQuery`. Both use `{queries:[...]}`; use `--json` for CLI machine output.
MCP lists and executes both when `OCTOCODE_BETA=true` or `local.beta` is enabled.
`graphBuild` writes snapshots and prunes older retained snapshots; `graphQuery` is read-only. Query the build's exact path even when pinning its ID.
Snapshots live under the current workspace's `.octocode/graph`. `keep` defaults
to three per scope; increase it when retaining comparison baselines. Query
continuations pin the snapshot, and every successful query reports freshness.
Call traversal defaults to high confidence. Verify consequential findings with
LSP and source reads. See [code graph architecture](../packages/octocode-native/docs/engine/CODE_GRAPH.md#agent-graph-tools-cli-and-beta-mcp).

`includeProvenance` defaults to `true`. Set it to `false` to represent proven unchanged UTF-8 mappings as `sourceOrigin:"identity"` (and the corresponding selection, binding or recipient origin). Their file, source digest and byte spans remain on the row. Transcoded, transformed, excluded and multi-source mappings retain explicit origins and source descriptors. This projection changes neither graph scope nor reachable rows; continuations preserve it.
