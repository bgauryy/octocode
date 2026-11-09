# Octocode tools reference

For a reader who needs each tool's **result behavior**: what a call returns, what a result proves, and the engine rules (language servers, Rust context, root selection, clone and graph handling) the schema does not carry. It does not repeat the other owners:

| Topic | Owner |
|---|---|
| Field names, defaults, ranges and field descriptions | the published schema: `octocode schema <tool> --view query` (generated from core) |
| Request and result envelope, rows, `next`, `related`, `hints`, paging | [Tool data contract](TOOL_DATA_CONTRACT.md) |
| Research routing, shape, recovery and measured cost | [Tool flow](../skills/octocode-research/references/tool-flow.md) |
| Availability, opt-ins, storage and caches | [Configuration](CONFIGURATION.md#tool-availability-cli-and-mcp) |
| `classification` and `clasify` | [Classification guide](OCTOCODE_CLASIFY.md) |

The catalog has 17 tools. The CLI lists all of them, `clasify` only with a classification key; MCP exposes a configured subset ([availability](CONFIGURATION.md#tool-availability-cli-and-mcp)). Listing a tool does not guarantee its provider or language server is ready; invocation reports those failures.

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
| `responsePagination{hasMore,next}` | The rest of the request: this response's later pages, then every row's walk on one handle | `responsePagination.next` until none remains |
| `rowPart{part,of}` | One row split across response pages | Read every part before using the row |
| `pagination{hasMore,…}` | A row's own list | `responsePagination.next`, else the row's `next.nextPage` |
| `contentPagination.<section>` | One `ghGetHistoryItem` collection; `countScope` says what `totalItems` counts | `responsePagination.next`, else that section's `next.nextPage` |

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

Hits always come from the default-branch index: `ref` re-reads them at that revision and never widens the search. GitHub indexes only files under its index limit, which every searched page states as `coverage.indexLimit.maxFileBytes` (384 KB on github.com and GHE.com, 5 MB on Enterprise Server), and skips archived repositories and most forks, so `empty` never rules out such a file: read it with `ghGetFileContent` `matchString`, or clone. Read continuations reach evidence omitted from the displayed hit window.

```json
{"queries":[{"keywords":["localSearch"],"owner":"bgauryy","repo":"octocode"}]}
```

### `ghStructure`

`operation: "refs"` lists tags grouped by tag prefix (`termcolor-1.4.1` stays apart from plain `13.0.0`). Within a group the newest version leads, and groups go by their newest version, so the order follows version number, not release date. Tags without a version come last.

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

Provider-native package coordinates remain values, not hidden operation selectors. Results use `packageName` and may offer related source/release reads. Without `version`, `resolve` names the latest stable release; a prerelease only when the registry has nothing else, with a warning (npm: the `latest` dist-tag). PyPI and NuGet rows carry the registry's `advisories`, PyPI and crates.io `yankedReason`. Other registries give no advisory data here, and an empty list does not prove a version clean: a CVE question belongs to [`securityCheck`](#securitycheck).

```json
{"queries":[{"ecosystem":"npm","operation":"resolve","packageName":"zod"}]}
```

### `securityCheck`

Known vulnerabilities (CVE, GHSA and other advisory ids) of a package version, a package URL or purl, an advisory, a commit or a GitHub repository, from OSV (`api.osv.dev`, operated by Google) with no token, and from GitHub: the repository facts that bear on trust (stars, last push, archived, missing or moved), the repository's published security advisories, GitHub's global advisory database (EPSS and CVSS scores) and where a commit lives. This section owns the tool: where it is offered, what each call sends, and what each status proves. [Security](SECURITY.md#advisory-egress) states the egress policy; [configuration](CONFIGURATION.md#all-settings-and-defaults) the setting.

| Surface | Offered | Sends anything |
|---|---|---|
| CLI | Always, with no setting | Only when you run a call |
| MCP, default | No: absent from `tools/list`, and the instructions say a vulnerability claim from memory is not evidence | Nothing |
| MCP with `OCTOCODE_SECURITY_CHECKS=true` (`tools.enableSecurityCheck`) | Yes; the instructions route known-vulnerability questions here | Only when an agent calls it |
| Repository settings (`.octocoderc`, `.env`) | Can turn it off for MCP, never on: the setting is home-only | Nothing |

Restart MCP after changing the setting. GitHub is read with the credential the other GitHub tools use; without one, anonymously, at GitHub's 60 requests an hour per IP, which a few rows spend. No other tool contacts OSV; `artifactSearch` carries `advisories` only from the PyPI and NuGet registries, and its silence elsewhere says nothing about vulnerabilities.

| `operation` | Takes | Asks | Leaves this machine |
|---|---|---|---|
| `package` | `ecosystem` (any OSV ecosystem the schema lists), `packageName`, `version`, `release` | For npm, PyPI, crates.io, Maven, NuGet, Go, Packagist and RubyGems (the contract's `registryReaders`): the public registry confirms it, then OSV at that exact version. Other ecosystems have no reader: OSV is asked as given and the row says `registryConfirmed:false`. Then GitHub: the repository the registry names (facts, cautions and its published advisories that name this package) and GitHub's global advisory database at the version. A `release` row is not sent to GitHub | The public name, ecosystem and, for a registry ecosystem, a registry-confirmed version, to OSV and to GitHub's advisory database; the registry's `owner/name` to GitHub. A package the public registry does not list, or a failed registry read, sends nothing (not checked), so a private package is not disclosed. An unlisted version is not sent: OSV is asked about the package only (`versionEvaluated:false`). For an ecosystem with no reader the name and version are sent as the caller wrote them, so a private name there is disclosed |
| `repository` | `repo`: `owner/name`, a `github.com` URL (its query and fragment are ignored) or a GitHub git remote; anything that is not a GitHub name is `invalidInput`, never a missing repository | GitHub's repository record (stars, forks, last push, archived, fork, default branch, license, redirected or missing) and every page of its published security advisories, each merged with OSV's record of the same id | `owner/name`, to GitHub. OSV receives the advisory ids only when GitHub says the repository is public. It returns facts, advisories and `cautions`, never a verdict |
| `url` | A package page or a purl; a `github.com` repository, commit (`/commit/<full hash>`) or advisory (`/advisories/GHSA-…`) URL | As `package`; a GitHub URL as `repository`, `commit` with its `repo`, or `advisory` | As the operation it runs. Any other URL names nothing and is refused before any request |
| `advisory` | `id`, such as `CVE-...` or `GHSA-...`; optional `repo` | OSV's whole record, withdrawn ones too. A GHSA id is also read on GitHub: its global record, and with `repo` the repository's own advisory, so one OSV lacks is still read whole | The id to OSV; a GHSA id to GitHub, with `repo` |
| `commit` | A full `commit` hash, optional `repo` | OSV at that hash alone. With `repo`, GitHub: whether the repository has the commit, its signature verdict, where it lives, and the repository's advisories | The full hash to OSV, even from a private repository. `repo` and the hash go to GitHub only; OSV never receives `repo` |

`release` applies to the distribution ecosystems that have releases (the contract's `distros`: Debian, Ubuntu, Alpine, Rocky Linux, AlmaLinux, Red Hat, SUSE, openSUSE); on any other ecosystem, Wolfi and Chainguard included (rolling, no releases), it is refused. OSV is asked for `<Ecosystem>:<release>` and `affected[]` keeps only entries of that release. OSV answers a release it does not name with the same empty list as a release with no advisory, so a 0-record answer for a release is disclosed as such: no `status:"empty"` and no `affected`, and `sources[].status` is `notCovered` with a `why`. A version of one of those distributions with no `release` and no advisory is answered the same way (not evaluated: pass `release`), never as none known; advisories OSV did list stand as a finding. With no `version` on an ecosystem with no reader, `affected` is absent: every version was asked about, none was evaluated.

`repository` describes the repository GitHub read: `owner`, `name`, `url`, `stars`, `forks`, `pushedAt` (last push to any branch, never `updated_at`, which a star or an archive moves), `archived`, `fork`, `disabled`, `defaultBranch`, `license`, `observedAt` and, on a package row, `provenance`: `claimed` (the registry says so) or `verified` (a signed attestation or a matching tag proves it). A claimed repository can add a caution, never clear one. `cautions` come from the contract, each with its `level`, `basis` and a `why` holding the raw values, the threshold and the day: `lowTraction` (fewer stars than `policy.lowTractionStars`; alone it is a note), `unmaintained` (last push more than `policy.unmaintainedAfterDays` days ago), `archived`, `deprecated` and `yanked` (the registry's word), `repositoryMoved`, `repositoryMissing`, `noSourceRepo` (the registry names no repository, so trust was not evaluated), and for a commit `commitNotInRepository` and `onlyInForkNetwork`. Both thresholds are echoed in the row's `policy`.

A commit read with `repo` states its GitHub facts in `sources[github].facts`: `exists`, `signature` (`verified`, `reason`; unverified is a note, not a finding), `heads` (branches whose head it is), `pulls`, `inTag`, `onDefaultBranch`, and `inBranch` (read only when no head, pull request, tag or the default branch holds it). `inRefsComplete` says whether every branch and tag was compared for `inBranch` or a history-derived `inTag`. A commit GitHub resolves through the repository but that none of those holds may live only in a fork (`onlyInForkNetwork`). `facts.repositoryAdvisories` lists the repository's published advisories OSV does not list for that commit or version (on a package row, only those naming the package), and `facts.globalAdvisories` the ids GitHub's database matches to the version and OSV does not list: GitHub's word, not counted in `answer`; read each with `advisory`.

GitHub reads that are refused (a rate limit, a missing or rejected token) leave `sources[github]` `failed` or `unavailable` with `errorCode` and a `why` naming the sign-in or the reset time; a failed source makes the row partial (`sourceFailed`) and draws no caution, and OSV's answer stays: unknown is not clean. `repository` does not ask OSV about the repository's head commit.

A Go package path is answered with its module's records; the ones whose stated imports exclude that package are listed, brief, in `moduleOnly.advisories` and are not counted in `answer` or `advisories`.

A row leads with the answer: `answer` in one sentence, then `affected`, `upgradeTo` (the lowest fixed version clearing every listed advisory, confirmed with OSV at that version) or `upgradeNote` (why none can be named). A package row also states the update: `latestVersion` (the registry's latest stable), `latestPublishedAt`, `currentPublishedAt`, `isLatest`, `outdated` (a newer release published after this one; false is not safe) and `upgradeKind` (`patch`, `minor` or `major` when both versions are plain dotted numbers, else `unknown`, never guessed). Each advisory is `id`, `cves`, `summary`, `severity` and its `source`, `fixedIn` and `link`; its `related` calls read every record whole, with details, ranges and references. GitHub's word rides beside it: `epss` (`percentage`, `percentile`), `cvss` as each source published it, `credits` (`login`, `type`) and, on a repository advisory, its `publisher`. `cvss[]` is `{type, vector, score?}`: `score` is the base score GitHub publishes with the vector; OSV publishes the vector only, so OSV's entries have none, and none is ever computed. `disputedBy` names aliases another database keeps for this package without listing this version (the databases disagree; the row still counts the record OSV lists), and `alsoAffects` the other packages a record names. The `advisory` operation states each source's record as its own row: OSV's, GitHub's global record and, with `repo`, the repository's. `sources[]` lists each source asked and its `status`.

| Row | Means |
|---|---|
| `affected:true` | OSV matched this version |
| `status:"empty"`, `advisories:[]` | None known to OSV, after a finished walk. A floor, not proof the version is safe |
| `versionEvaluated:false` | The registry does not list this version, so OSV was asked about the package only |
| `commit` with `status:"empty"` | None known to OSV for this commit. OSV answers the same for a commit it does not track, so `affected` is absent |
| `status:"error"`, no `advisories` key | Not checked: the registry or OSV failed, or the package is not on the public registry |
| `isPartial:true` | The walk stopped early (`next.nextPage` reads on), or a source failed (`sourceFailed`) |
| `sources[].status` | `found`, `noneKnown` (that source only), `partial`, `failed` (asked, no answer; never clean), `unavailable` (not asked, for want of a token or quota), `notCovered` |

```json
{"queries":[{"operation":"package","ecosystem":"npm","packageName":"lodash","version":"4.17.20"}]}
```

## Local code tools reference

Fields: `octocode schema <tool> --view query`. Paths resolve against the workspace and must satisfy the read policy. Default exclusions and ignored paths are disclosed when relevant; a related broader search is a new scope, not proof that the first was complete.

### `localSearch`

Every shown match keeps its source coordinates. Count and scope disclosures describe what was actually searched.

```json
{"queries":[{"matchString":["registerHandler"],"path":"src"}]}
```

### `astSearch`

A syntax match does not prove a runtime call or reference. A scan cut by its time budget continues with `next.expandScan` after the files it evaluated; no file count or walk ceiling ends a scan. Under a ranked order (`sort.by: matchCount`, or `order: desc`) a cut window lists no file yet: the window that ends the scan lists the whole scope in that order.

```json
{"queries":[{"operation":"match","parser":"typescript","path":"src","rule":{"pattern":"eval($X)"}}]}
```

### `structureSearch`

`next.nextPage` continues through every page and internal scan window, so no walk ceiling ends a listing; `sort` orders each scan window before paging, so follow every continuation to finish the selected scope. A listing groups file entries by directory; read source separately. `view:"paths"` shows names only and reads no entry's metadata (unless a filter or sort needs it), so its `summary` has no `bytes` and a directory with its own group is named by that group alone. Entries without the sort key (a directory or link for `sizeBytes`; a directory, binary or unreadable file for `lineCount`) rank last in both orders.

```json
{"queries":[{"maxDepth":2,"operation":"tree","path":".","view":"paths"}]}
```

### `localFetch`

A symbols outline describes syntax, not resolved identity, and data outlines summarize containers rather than list every nested scalar. A code outline renders the same declarations `astSearch` symbols lists, at any size a whole read admits: each declaration's name row (a row over 1,000 characters lists the declaration heads it holds), the member rows of a class, struct, impl or module body, and import runs as one row. When the file has no outline, a compact view that reads its grammar stands in, named in `viewFallback`; when compact could only drop whitespace or is not smaller than the content view, the row is `symbolsUnsupported` with both reasons and a related read of the lines. A compact read's first page says how many bytes its view keeps of what the content view serves for the same lines, gutters included; a compact view that is not smaller is not served: the content view is, with `viewFallback.reason` `not-smaller`. Markdown outlines recognize ATX (`#`) headings; underline-style Setext headings require content reads. A binary source produces a typed error. Secret scanning happens before projection, including range reads.

Compact reads remove comments and spare whitespace where a grammar in the build parses the file. Strings, regexes, template literals, what the grammar could not read, and comments tools read (`//go:build`, cgo preambles, `// @ts-ignore`, `# type: ignore`) stay byte for byte, and every kept line carries its source line number. For CSS and SCSS the language's own comment and string rules are data on the grammar's row: a comment goes only where the grammar and those rules agree (a `//` line in `.css` is rule text and stays; in `.scss` it is a comment), and every string, a backslash-continued line included, stays byte for byte. Without a grammar, or when the grammar cannot read most of the file, compact drops only blank lines and trailing whitespace, and `viewFallback.reason` says so (`whitespace-only-no-grammar`, `whitespace-only-syntax-errors`, `whitespace-only-interrupted`). Markdown, YAML, TOML and JSON stay as they are, comments and blank lines included, because those bytes can be values, so their compact read serves the content view (`not-smaller`); `view:"symbols"` still outlines YAML keys. Use LSP to resolve a call's definition.

```json
{"queries":[{"path":"src/index.ts"}]}
```

## LSP tools reference

### `lspSearch`

Fields: `octocode schema lspSearch --view query`. A missing or still-indexing server does not prove absence. A row's `role` is `declaration`, `call`, `import` or `reference`.

Each row's `lsp.level` names which source answered. The first source that can answer does; a row answered below the first applicable source carries `lsp.fellBackFrom` with that level and the reason:

| `level` | Source | Taken when |
|---|---|---|
| `ide` | The editor's own language features, through the Octocode VS Code extension's bridge (`lsp.ide.name` names the editor) | An editor with the bridge on (`octocode.ideBridge.enabled`, the default) has the file inside one of its workspace folders. It gets half the call's deadline; a timeout, an unsupported operation or an error falls to the next level |
| `server` | A language server Octocode starts: configured in `lsp-servers.json`, a managed install, or found on `PATH` | A server for the file's language is found and starts |
| `syntax` | The file's tree-sitter grammar: declarations, references and calls by name, without type resolution | No server is available for a file whose grammar is in the build. Answers are candidates, like AST matches; diagnostics are parse errors only |
| `text` | Nothing structural: the row carries the reason and, when a server can be installed, the install hint | No server and no grammar. Use `localSearch` and `localFetch` |

A policy refusal (fetched code, build scripts) is not a fallback: the row stays refused at its level. Server discovery is cached per workspace and extension, and resolved again when `lsp-servers.json`, the managed install, the language's toolchain state or a `PATH` directory searched up to the found server changes. `next.nextPage` walks one importer window's locations and `next.nextImporterWindow` enters the next window: follow both, and the other hierarchy continuations, to reach every row.

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

`classification` is supported by `localSearch`, `localFetch`, `ghSearchCode`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem` and `structureSearch`. Search and read tools judge each hit or line; `ghSearchHistory` judges each row's title and body; `structureSearch` judges each listed path; `ghGetHistoryItem` judges a pull request's or issue's comments, reviews and review comments, so request those sections. The receipt distinguishes selected, related, excluded and unknown units; unknown is never a negative decision. Paging and absence rules (`next.nextCandidatePage`, `next.unfiltered`, `next.unfilteredUnknown`): [tool flow](../skills/octocode-research/references/tool-flow.md#scout-judge-what-you-have-not-read); the 64-unit page budget and the 0.5 keep threshold are in the schema.

### Rust build context

`rustContext` fields and limits are in the schema. A field left out keeps the effective value (the configured server options, else rust-analyzer's default); `features` are deduplicated and sorted for identity, and `target` is otherwise the server's Cargo environment.

- In a crate with a build script that did not run, an incoming answer missing files that spell the name is partial (`buildScriptsOff`) and, where the host allows build scripts, offers a `related[]` lead with `rustContext.buildScripts:true`: the same query in that context. One note states the cost: it executes the workspace's build scripts (repository code) in a separate rust-analyzer (~2 GB). Without that opt-in (below) the row keeps `buildScriptsOff`, offers no lead, and its note names the opt-in.
- rust-analyzer loads only the package directory, so an `include!` of a file outside it never resolves: an answer missing files that spell the name through such an include is partial (`includeOutsidePackage`) with a warning naming the file; no `rustContext` recovers it; check those uses lexically with `localSearch`.
- A file that several files declare as a module (`#[path = "support/x.rs"] mod x;` in a bench and a test) is compiled once per declaring crate, and rust-analyzer answers for one copy: an incoming answer missing files that spell the name is partial (`sharedModuleFile`) with a warning naming the declaring files; check the other copies' uses lexically with `localSearch`.
- A context is an overlay: it changes only the fields it sets, and only where the value differs from the effective one. Everything else, `cfg(test)` code included, stays as the default server analyzes it, so `{}` or a context equal to the defaults reuses the default server. Build scripts and proc macros run only when `buildScripts`/`procMacros` is true and the host allows them; enabled providers run workspace code, and the context is not a sandbox.
- Build scripts and proc macros are opt-in, whatever the client or its mode: a query whose `rustContext` sets `buildScripts:true` or `procMacros:true` gets `errorCode:"policy"` and starts no server, while every other query keeps the default rust-analyzer, which runs none. To allow them, set `lsp.allowBuildScripts` (`OCTOCODE_LSP_ALLOW_BUILD_SCRIPTS=true`) in your home settings or the process environment; a workspace `.octocoderc` or `.env` cannot, so a repository cannot allow its own build scripts ([external processes](SECURITY.md#external-processes)). Allowing it lets any `lspSearch` call run the build scripts and proc macros of the workspace it names, so pair it with a client that asks before such calls.
- rust-analyzer supplies cfg-selected definitions, declarative macro expansion, and enabled build-script or proc-macro results; `astSearch` gets no compiler expansion. A disabled provider can explain an empty answer for a declaration a normal Cargo build generates.
- Different effective settings use different pooled clients. `rustContext.fingerprint` identifies the normalized requested settings and is part of pagination, so a changed context restarts pagination. It does not pin sources, Cargo configuration, toolchain, environment, or generated artifacts, and is not a reproducible-build identifier. See the [engine lifecycle contract](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md#rust-context) and [rust-analyzer configuration](https://rust-analyzer.github.io/book/configuration.html).

### Root selection

Without `workspaceRoot`, the root is the nearest ancestor of the file holding its own language's project file (TypeScript and JavaScript: `tsconfig.json`, `jsconfig.json`, `package.json`, `pnpm-workspace.yaml`, `yarn.lock`; Python: `pyrightconfig.json`, `pyproject.toml`, `setup.py`, `setup.cfg`, `Pipfile`, then `requirements.txt`; Go: `go.mod`; Rust: `Cargo.toml`; Java: `pom.xml`, `build.gradle(.kts)`, `settings.gradle(.kts)`; Scala: `build.sbt`, `build.sc`, `build.mill`; C#: `*.sln`, `*.slnx`, then `*.csproj`; C family: `compile_commands.json`, `build/compile_commands.json`, `compile_flags.txt`, `.clangd`), else its repository (`.git`), else, for the C family, the top of its chain of build files, else the nearest generic marker (`Makefile` or another language's file). A marker of another language or a subdirectory's `Makefile` names no project of this language, even inside `WORKSPACE_ROOT`, widened for Rust to the Cargo workspace root, for TypeScript to the directory of the governing `tsconfig.json`/`jsconfig.json`, for Go to a `go.work` directory above, for Python to a `pyrightconfig.json` (or `pyproject.toml` with `[tool.pyright]` or `[tool.uv.workspace]`) above, and for Java to the Maven parent whose `<modules>` list the module or the Gradle settings that include it; with no marker anywhere, the file's own directory, never the process cwd. An explicit `workspaceRoot` must contain the file. A `workspaceRoot`-only query infers its language from the root's markers (`tsconfig.json`, `Cargo.toml`, `go.mod`, `pyproject.toml`, `setup.py`, `requirements.txt`, `go.work`, `compile_commands.json`, `.clangd`, `CMakeLists.txt`, `meson.build`, `pom.xml`, `build.gradle`, `build.gradle.kts`, `build.sbt`, `jsconfig.json`, `package.json`).

### Root coverage and workspace symbol limits

A row is partial (`serverUnanswered`) when its server cannot have loaded every file that may hold a use. Where no `.git` or workspace file says where the project ends (an unbounded scope), every `references`, `callers`, `implementation` or `subtypes` answer is partial: the row names the project directories beside the server's root, tells you to widen (not narrow) `workspaceRoot` to their parent, and its text-search lead covers those directories. With a `go.mod` outside the opened Go workspace (no `go.work`), the row names the module the server did not load.

#### Workspace symbol limits

A `workspaceSymbol` answer whose count equals its server's cap is partial (`workspaceSymbolCoverageUnknown`) and says the server capped it, because the rest are unknown. The cap is the server's own: gopls 100, rust-analyzer 128 (or `/workspace/symbol/search/limit` when configured), clangd 100 (or its `--limit-results`; 0 means no limit), tsgo 256. The advice is to narrow `symbolQuery`; for tsgo also to route `.ts` to `typescript-language-server` in `lsp-servers.json`.

### Fetched code

No language server starts on code Octocode fetched: a file or `workspaceRoot` that resolves, symlinks followed, into its `ghCloneRepo` or `ghStructure materialize` checkouts or URL captures gets `errorCode:"policy"`, because a server can run code from the repository it opens ([external processes](SECURITY.md#external-processes)). So does a root that holds any of that code, inferred or named: csharp-ls and jdtls enumerate dot directories, so `$HOME` qualifies once something is fetched. Pass a `workspaceRoot` that does not hold it, such as your project under `$HOME`; a root that holds no fetched code keeps its server. Text (`localSearch`) and syntax (`astSearch`, `structureSearch`) tools read that code without running anything, and so do the graph tools (`graphBuild`, `graphQuery`) when Octocode runs from inside that directory: `graphBuild` takes only a path inside the workspace. Prewarm and `lspSearch` leads skip it, and `octocode lsp-server status` reports the refusal. To opt in, set `lsp.trustFetchedCode` (`OCTOCODE_LSP_TRUST_FETCHED_CODE=true`) in your home settings or the process environment; a workspace `.octocoderc` or `.env` cannot. A local checkout keeps its server.

### Server fidelity and the no-fallback contract

Every `lspSearch` operation, including `documentSymbols`, needs a real server; without one the row is `serverUnavailable` with recovery through `localSearch` or `localFetch` ([semantic boundary](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md#semantic-boundary)). Markdown has no built-in server route: configure one, or use `localFetch view:"symbols"` for syntax-only heading outlines. There is no same-file-only `references` path. `diagnostic` on a server without pull diagnostics waits up to 1.5 s for a push after the document sync; a server that publishes only when its diagnostics change (typescript-language-server) keeps its last report, which is returned with a warning that it predates the last edit.

### Subtypes and implementations without an answer in time

`subtypes` on a server with no type hierarchy asks `textDocument/implementation`; `implementation` on a class, interface, struct or trait asks it directly. For both, a syntax scan of the files that spell the name (declarations whose `extends`/`implements` clause names it) runs at the same time. For `subtypes` it adds no wait; for `implementation`, a server answer also waits for the scan, and the declarations it found that the server left out (a function-local impl, say) follow as rows labelled `source: "native-graph-facts"`, partial (`partialReasons: ["scanOnlyImplementations"]`, no retry); page 1 states their count once (the `source` label's meaning is in the output schema). The server's rows win when they arrive: while the scan runs and for up to 20 s after it finishes. If the request fails (timeout, crash, rejected) or is still silent after that grace, the scan's rows are the answer, labelled `lsp.source: "native-graph-facts"`, partial (`partialReasons: ["serverUnanswered"]`) with a `related` rerun to ask the server again, and a note that they are unverified (the base is matched by name, not resolved to the declaration) and direct only (run the operation on each to descend). Files the scan could not read are named in a warning, add `heritageUnread` to `partialReasons`, and get a `related` text search over them. A server answer is labelled `lsp.source: "implementation"`; for `subtypes`, direct and transitive subtypes are not told apart. rust-analyzer answers a trait with its impl blocks: each trait impl (`impl Trait for Type`) is a direct subtype row with `kind: "impl"`, named by its self type, and has no subtypes of its own. A `subtypes` walk checks the anchor type's candidates 200 at a time: past the first window the row is partial (`hierarchyCandidateLimit`) and its last page offers `next.nextCandidatePage`; a deeper type with more than 200 candidates is checked 200 at a time too: the walk checks its first window, adds `hierarchyCandidateLimit`, and resumes it with `next.continueWalk*` as the anchor of its own walk, whose candidates page the same way. `implementation` on a function, method or other member has no scan: a timeout stays an error with the retry tip and the astSearch/localSearch route.

### TypeScript backends

1. An `lsp-servers.json` entry for the extension ([custom servers](#custom--bring-your-own-servers)); it always wins and gets no substitute.
2. `tsgo`, for TypeScript (`.ts`, `.mts`, `.cts`, `.tsx`) only: the workspace's own `node_modules/.bin/tsgo` when `lsp.trustProjectConfig` (`OCTOCODE_TRUST_PROJECT_LSP_CONFIG=true`) is on, else the `@typescript/native-preview` platform executable in Octocode's install tree (`octocode-mcp` and the `octocode` CLI depend on it). JavaScript stays on `typescript-language-server`, because tsgo drops constructor functions and `this` expandos. tsgo returns at most 256 workspace symbols (see [workspace symbol limits](#workspace-symbol-limits)); its capped answer advises narrowing `symbolQuery`, or routing `.ts` to `typescript-language-server` in `lsp-servers.json`.
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

Clone a verified repository/ref, then use the returned path with localSearch, structureSearch or astSearch; `lspSearch` needs [an opt-in](#fetched-code) there. Keep the materialization scope in the claim.

### `graphBuild` and `graphQuery`

Build or reuse a syntax graph with `graphBuild`, then explore the exact scope with `graphQuery`; use `--json` for CLI machine output. They are the only graph surface: the CLI has no separate graph command. Both are always available as CLI commands; [configuration](CONFIGURATION.md#tool-availability-cli-and-mcp) controls MCP exposure. `graphBuild` writes snapshots and prunes older retained ones; `graphQuery` is read-only. Query continuations pin the snapshot, and every successful query reports freshness.

Call traversal defaults to high confidence. Verify consequential findings with LSP and source reads: `impact` rows reached through imports are module-level candidates, so confirm a symbol change breaks them with `lspSearch` references. For bulk evidence, send `responseChars: 50000`, list up to the schema's maximum of sources as `witnesses`/`unresolved`/`symbols` `target`, and filter `unresolved` with `reasons` before paging. Scope, snapshot and coverage rules: [tool flow](../skills/octocode-research/references/tool-flow.md#graph); architecture: [code graph](../packages/octocode-native/docs/engine/CODE_GRAPH.md#agent-graph-tools-cli-and-beta-mcp). `includeProvenance` is defined once in [Minimal by default](TOOL_DATA_CONTRACT.md#minimal-by-default).
