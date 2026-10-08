# Changelog

## [Unreleased]

### Fixed — clasify
- **Oversized `ranges` reads stay reachable.** A line window too large for
  `maxChars` now fails with `classificationContextTooLarge` plus a `hints.read`
  of its exact lines, and a `ranges` read continues past it through
  `next.clasify` even when nothing else in the call was judged. Covered for
  `localFetch` and `ghGetFileContent` ranges.
- **An open walk's `best` says it is page-local.** While `next.clasify`
  remains, `best` ranks only the pages judged so far; in 4 of 7 measured
  multi-page targets that first window was a confident near miss. Such a
  response keeps every `best` row and adds one `hints.text` tip: if the read
  lacks the answer, run `next.clasify`.
- **Identifier sentences skip the provider.** A `locate` ask whose only
  content is one identifier or quoted literal (`find escapeRegExpCharacters`,
  `where is Type_instantiation_is_excessively_deep_and_possibly_infinite
  reported`) now short-circuits like a bare identifier: no read, no provider
  call, a `hints.textSearch` lead. Described asks still run.
- **No literal hint for context identifiers.** A described ask that only
  mentions a type (`where does QuerySet filter after a slice`) no longer gets
  the "Target names `QuerySet` … use localSearch" tip or a `hints.textSearch`
  for it. The short-circuit and the tip use one classifier.
- **One literal rule for the large-read handoff.** A paged `localFetch` read
  whose `mainGoal` is described but mentions an identifier
  (`Where bulk_update refuses pk changes`) now gets the clasify locate offer;
  only a literal goal (`find bulk_update`) gets the `textSearch` lead.
- **`hints.textSearch` reaches every literal.** Several literal targets share
  one escaped `regex:"rust"` alternation (`step_17|step_23`) instead of a
  search for the first; a matrix of only literal targets short-circuits even
  when the literals differ.
- **Scout leftovers.** `relevant`/`sufficient` over a path list
  (structureSearch, ghStructure, astTopology) keeps its one list verdict and
  adds a `hints.text` tip to ask `choice` over the paths. Leads in error hints
  drop `debug:false`, the brief, and the read's inherited `pageSize`. Every
  page read of a paged `localFetch` file now pins the same `snapshot`, page 1
  included.

### Breaking — configuration
- **One name per setting.** Removed `ENABLE_LOCAL` (use `OCTOCODE_ENABLE_LOCAL`)
  and `OCTOCODE_JEV_KEY` (use `OCTOCODE_CLASSIFICATION_API`).
- **GitHub token variables:** `GH_TOKEN`, then `GITHUB_TOKEN`. `OCTOCODE_TOKEN`
  and `GITHUB_PERSONAL_ACCESS_TOKEN` are no longer read; `GH_HOST` is no longer
  declared (`github.apiUrl` selects the host).
- **Language servers:** the nine `OCTOCODE_*_SERVER_PATH` variables are gone; an
  `lsp-servers.json` entry (home, `lsp.configPath`, or a trusted project) replaces
  a built-in server, e.g. `".ts": {"command":"tsgo","args":["--lsp","-stdio"],"languageId":"typescript"}`.
  Assembly servers come only from that file.
- **`tools.family` / `OCTOCODE_TOOL_FAMILY` removed;** use `tools.enabled` /
  `tools.disabled`.
- **Now config keys (same env names):** `lsp.autoInstall` (`prompt` default,
  `off`, `auto`), `lsp.cacheDir`, `lsp.trustProjectConfig` (home-trusted), and
  `lsp.prewarm` (`targeted` default, `all`, `off`; `OCTOCODE_LSP_PREWARM=1` is
  no longer accepted, use `all`).
- **Moved under `storage`:** `session.enableStats` → `storage.stats`;
  `cloneCache.ttl` / `maxSize` / `maxClones` → `storage.cloneCache.*` (env names
  unchanged).

### Breaking — `@octocodeai/config` package
- **Public API trimmed to what Octocode uses:** `CONFIG_FIELDS`, `DEFAULT_CONFIG`,
  `ENV_TOKEN_VARS`, `INTERACTIVE_EXECUTION_TIMEOUT_SECS`, `RuntimeSurface`,
  `applyOctocodeEnv`, `configFieldEnvNames`, `contractDriftAllowed`,
  `contractDriftMessage`, `devOverridesAllowed`, `getConfigFilePath`,
  `getOctocodeHome`, `getProjectConfigFilePath`, `loadOctocodeEnv`,
  `propagateOctocodeEnv`. The TypeScript settings resolver, validator, and
  `.octocoderc` loader are gone (the native runtime is the one resolver), as are
  the token helpers and runtime-surface state.
- **The `octocode-config` bin (`npx @octocodeai/config`) is removed;** use
  `octocode config` and `octocode config check KEY`.

### Breaking — CLI
- **`scheme` is now `schema`** (`octocode schema`, `octocode schema <tool> --view query [--select F=V]`).
  Indented JSON on a terminal, one line on a pipe; `--compact` and `--pretty` are gone.
- **Output follows the terminal.** Tool commands print the rendered text MCP
  clients read on a terminal and single-line JSON on a pipe; `--json` forces
  JSON. Errors follow the output: the `octocode.toolError` envelope on stdout
  in JSON mode (mistyped commands included, with did-you-mean names), text on
  stderr otherwise. `--input -` reads the query from stdin.
- **Removed duplicates:** `showConfig` (use `config`), `--json-errors` (use
  `--json` or a pipe), `--redact-emails` (use `OCTOCODE_REDACT_EMAILS` or
  `output.redactEmails`), `--no-color` (use `NO_COLOR`), `--pretty`, bare
  `auth` and `auth --json` (use `auth status [--json]`), install ids `claude`
  and `vscode` (use exact ids from `install --list`), skill platforms
  `claude-desktop`, `shared`, `common`, `agents`, `codex-native`, `skill help`,
  and `lsp-server remove`.
- **`config` subcommands:** `config set KEY VALUE` / `set KEY --stdin`,
  `config unset KEY`, `config check KEY` replace `--add`, `--value-stdin`,
  `--remove`, and `--check`. `config` now lists config warnings; unknown config
  keys no longer warn on every command.
- **`auth status --json`** is `{authenticated, verification, username, hostname, tokenSource}`.
- **Bare `octocode`** prints the command reference on a terminal and the
  `schema` catalog on a pipe; root help no longer appends agent instructions.
- **`skill`:** retired-skill migration is gone (`skill check` no longer reports
  or removes `octocode-clasify` installs, and its JSON drops `retired`);
  `skill install --workspace` is an unknown option (it applies to `skill check`);
  `skill list --json` env params use the `skill check` row shape.
- **`OCTOCODE_NATIVE_BIN` is a development override:** like MCP's
  `OCTOCODE_NATIVE_BINDING`, the shipped CLI ignores it unless `NODE_ENV` is
  `development` or `test`.

### Breaking — `octocode-mcp` package
- **One export path:** the `./types` and `./public` subpaths are removed; import
  from `octocode-mcp`. The runtime types (`NativeCatalog`, `NativeCatalogTool`,
  `NativeRuntimeOptions`, `ClassificationProbe`) come from
  `@octocodeai/octocode-native/runtime`; `NativeCatalogTool.shortDescription`
  and `NativeCatalog.grammarCapabilities` are no longer declared.
- **Scripts:** `mcp:contracts`, `mcp:package`, `test:contracts`, `build:ci`, and
  `build:publish` are removed (use `test`, `verify`, `build`); new
  `test:boundary` and `test:acceptance` run the built-package boundary checks.
- **Desktop extension:** the unused `npm_registry` setting is removed (the
  registry comes from each `artifactSearch` query).

### Breaking — `@octocodeai/octocode-native` runtime ABI 5
- **Per-request GitHub token:** `execute` and `executeMcp` take an optional 4th
  argument `{ githubToken }`. A supplied token is the request's only GitHub
  credential: no env, stored, or `gh` fallback, never echoed, never added to
  the shared rejected-token set. Cache, rate limits, and page replays are
  partitioned by it, and an auth failure hints the host to re-authenticate
  (not `octocode auth login`). Unknown option keys and a blank token are
  `invalidInput`. Without the argument a request resolves credentials as before.
- **ABI 4 → 5.** Hosts check `NativeRuntime.abiVersion`; an older addon fails
  closed at startup.

### Breaking — tool exit codes and output shapes
- **One exit class per error code** (`docs/OCTOCODE_TOOLS.md` exit table).
  Scripts that read exit codes see: `outsideAllowedRoots` and
  `pathValidationFailed` on local tool rows 5 → 2; artifactSearch
  `authentication` 5 → 4; `rateLimited` (artifactSearch, ghCloneRepo git) 5 → 7;
  astRewrite `languageRequired` 5 → 2; localSearch bad glob or file type
  (`invalidInput`) 5 → 2; clasify with every resource `notFound` /
  `versionNotFound` / `anchorUnresolved` 5 → 3; GitHub bad ref (`notFound` plus
  a ghStructure `refs` lead) 2 → 3.
- **ghSearchHistory commit filters:** the typed `author` and `committer` fields
  are gone; use `qualifiers:"author:x committer:y"`.
- **lspSearch Rust cfg-gated rerun:** the lead `hints.allFeatures` is now the page
  `next.expandFeatures`, which reaches the missing references.
- **ghStructure entries:** `files: ["a.py (22, 2026-09-23)"]` and
  `folders: ["sub (2026-09-23)"]` replace the bare names plus the `updated` map.
  The first 100 entries are dated; the rest come from `next.expandDates`
  (`pageSize:100` rows at the pinned SHA).
- **astSearch `symbols` and lspSearch `documentSymbols`:** members nest in their
  container's `members`; `shared` states a `kind`/`exported` that 2+ members
  share; only a member whose container is off the page keeps `parent`.
- **Symbols outline entries (P1):** in astSearch `symbols` and lspSearch
  `documentSymbols`, a declaration without members is now one entry string,
  `"<symbolName> (<line>[-<endLine>][, <kind>][, doc <docStartLine>][, exported][, <key>=<value>]…)"`
  (the structureSearch entry grammar: the last ` (` opens the fields;
  `exportedAs`, `startLine`, `column`, `parent`, `parentLine` follow as
  `key=value`). Containers stay `{symbolName, kind, line, …, shared?, members}`
  objects, and `members` is required on them. Readers of `row.symbolName` /
  `row.line` on leaves must parse the entry
  (`docs/TOOL_DATA_CONTRACT.md` location rows). A/B: −35% tokens on 8 real
  outlines with 100% held-out replay accuracy on Sonnet and Haiku (30 runs),
  lossless round trip. Envelope `shared` never hoists `symbolName`.
- **Hints** are never clipped; every source fits 120 characters.
- **Leads:** artifactSearch leads no longer repeat the row's `verification`. A
  complete single-hit answer (no page, not partial, one evidence entry) carries
  1 lead instead of up to 2; an issue read ranks its fix PR (`readPullRequest`)
  before `readDiscussion`.
- An astSearch `syntaxTree` partial row has no `errorCode` (`isPartial` and the
  diagnostics say it). 68 old error codes are retired (deny-list in
  `skills-dev/octocode-dev/scripts/retired-names.json`).

### Changed
- **Faster repeat and continuation calls (output unchanged):**
  - GitHub repository, issue, pull-request and commit searches are cached for
    60 seconds like code search; a repeat sends no request and skips the ~2 s
    search spacing. A page GitHub marks incomplete is never cached.
  - astSearch `match` continuations reuse page 1's scan while every scanned file
    is unchanged (269 → 9 ms for page 2 on a large tree); a repeated directory
    `symbols` outline is served from the same memo.
  - localSearch pages hash files while searching them (no second read to store a
    page) and a stored page re-checks only the files it reads or shows.
  - structureSearch `files` stats only entries that pass its filters.
  - `graph ingest` on an unchanged tree compares size and time before hashing a
    source (new optional `FSTM` section in `graph.bin`).
- **Language servers stay up through one slow call:** a request that uses its
  whole timeout gets `$/cancelRequest` and the server stays; a second
  consecutive timeout retires it. After a readiness `timeout`, the next call
  waits for readiness again instead of failing until idle eviction.
- **Warm `lspSearch` across CLI calls.** The first call of a workspace starts a
  private per-workspace server (`<home>/run`, owner-only socket, 10-minute idle
  exit); later calls skip the language server cold start (TS hover ~3 s → ~0.1 s).
- **`lspSearch` importer pages.** TypeScript/JavaScript `references` and
  `callers` now reach importer candidates past the 24-file cap: the sorted
  candidate list is cut into 24-file windows, `importerPage` selects one, and
  the last location page of a window carries `next.nextImporterPage` (with the
  candidate digest as `snapshot`). Each candidate is verified in exactly one
  window and each recovered row is listed once; importer pages after the first
  leave out the server's own answer. A changed candidate list is
  `staleSnapshot` with `next.restart`. `importerScanCapped` is no longer a
  terminal limit (`aliasScanCapped` still is). With `callers` `depth` > 1, only
  level-1 callers are windowed.
- **`lspSearch documentSymbols`** lists every nested symbol (no
  `unlistedNested` count), pages 100 symbol rows by default (`pageSize` default
  is 40 for locations), and names TypeScript `type` aliases `type`.
- **`skill list` and `skill check`** share one status (`ok`, `not-installed`,
  `stale`, `broken`); a link into a source checkout is the user's and is never
  marked stale; a stored or `gh` GitHub login satisfies the token hint.
- **Minimal responses by default.** Rows carry the answer, open pagination,
  `next.*` continuations, warnings, and errors; `debug:true` adds scan stats,
  provider receipts, snapshots, `cache`, and fields such as `data.lsp`.
- **Leaner text output.** A single-row YAML response drops the
  `results`/`index`/`data` wrapper, path-only search rows render as compact
  strings (`path` or `path (count)`), and `localFetch` numbers lines with an
  `rg -n` style gutter (`279:`). JSON and `structuredContent` keep the envelope.
- **`followUp` continuations.** Every `next.*` query except `next.clasify`
  carries `followUp:true` and inherits the brief; each new query row still
  needs its own `goal` and `reasoning`.
- **`defaultExcludes`** on `localSearch`, `structureSearch`, and `astSearch`
  controls the built-in dependency/build-output excludes. Sensitive paths stay
  denied and `.gitignore` still applies without `noIgnore`.
- **PR review.** `ghGetHistoryItem` pull requests list changed files as compact
  rows (`"M +3 -1 [!flag ]path[ <- old]"`) grouped by directory; `fileFilter`
  (paths/globs, status, `minChanges`) narrows files and patches; `matchString`
  returns only matching hunks with `matchContext` (0–10) and a
  `next.readFullPatches` step; large PRs offer a `next.findInPatches` template
  (`confidence:"low"`); patch reads carry a slim identity header.
- **clasify.** A process-local judgment cache with in-flight dedupe, the
  `sufficient` preset, and a use/skip rule (behavioral target across ≥2 files;
  skip identifiers, literals, and PR filters). The search handoff carries the
  three top-ranked files plus a `prefilter`.
- **Ranking.** Identifier searches rank the declaring file first.
- **MCP instructions** stay within 2,000 characters; the grammar inventory is
  CLI-only. `structureSearch` defaults to `operation:"tree"`.
- **Local bench suite** (`octocode-local-testing/bench`, not published):
  head-to-head validation of local and GitHub tools against expert shell and
  `gh`, plus a feature-claims audit.

### Removed
- **`octocode-clasify` skill merged into `octocode-research`.** Clasify
  admission, request shapes, question types, Scout/Judge and result handling
  live in `octocode-research/references/clasify.md`; the bundle now ships 15
  skills. `skill check` reports installed copies or dangling links of retired
  skills (exit 1) and `skill check --fix` removes them; `skill install`/`info`
  of a retired name point to its replacement. Manual cleanup:
  `octocode skill remove octocode-clasify`.
- **Awareness retired.** The Awareness package, CLI, repository skill and hook
  registrations are removed, together with its plan projections, work ledger,
  shared memory, automatic history capture and `/rewind`.
  `@octocodeai/octocode-agents-communication` now owns session identity, path
  leases, messages and delivery (wake policy in its skill and README). Existing
  Awareness SQLite files and `.octocode/.localGit` archives are left on disk and
  not imported; this repository's data moved to
  `.octocode/retired/octocode-awareness-2026-09-25/data` (SHA-256 checked in
  `preservation.json`). Reissue unfinished prompts; do not reuse an old
  database path for the new stores.

- **Native addon credential methods.** `NativeRuntime` no longer exposes
  `storeCredentials`, `getCredentials`, `deleteCredentials`, `refreshAuthToken`,
  or `getTokenWithRefresh` (no interface called them; the last one handed a raw
  token to JavaScript). Use `octocode auth login|logout|status`.

### Fixed
- **Typed input errors on every surface:** a rejected query (schema-invalid,
  malformed or missing JSON) answers with the `octocode.toolError` envelope
  carrying `errorCode:"invalidInput"` and its repair `details`: CLI stdout in
  JSON mode, and MCP `structuredContent` beside the `isError` text (which now
  ends `(errorCode: invalidInput)`). Before, MCP returned text only and neither
  envelope named the code. The MCP `run` dispatcher's unknown-tool error uses
  the same envelope.
- **ghGetHistoryItem patch-walk hops page by rows:** `continuePatch` and a patch
  read's `nextFilePage` ask for `responseScope:"rows"` with their doubled
  page, so a hop whose row outgrows the page splits into structured `rowPart`s
  (its walk lead on the last part) instead of text windows that emptied
  `results` and hid the walk.
- **Issue comment-body hops list only unfinished bodies and keep their page:**
  `continueCommentBody` no longer re-lists comments an earlier window delivered
  whole (PR hops did this already; an empty body now counts as delivered on PR
  hops too). The hop keeps its first window's `body` section to size the page
  and pages exactly that window's comments; before, a hop that read the
  discussion without the issue body fit more comments, showed some first at a
  later offset (their opening text never read), and its `nextCommentPage`
  skipped them.
- **clasify list walks judge every candidate:** a repository, history, or
  package-discovery list whose `maxChars` fits only some candidates resumes
  `next.clasify` at the first deferred one (a `page`/`pageSize` that starts
  there) instead of marking the rest `classificationBudgetSpent` and moving on
  to the next page.
- **clasify states a GitHub file's source once:** pages that share
  `source {ref, path}` drop it; the resource states `path` and the new `ref`.
- **Page totals:** a compare's commit `pagination` states `totalItems` (GitHub's
  `totalCommits`) and `totalPages`; every byte page states
  `pagination.totalBytes` (before, only `debug:true` showed the total, as
  `sourceBytes`).
- **npm release leads are checked:** a `gitHead` GitHub reports missing no
  longer becomes a `viewReleaseSource` lead that fails; the row offers the
  default branch (`viewRepo`, `verification:"defaultBranch"`) and warns with
  the dead commit.
- **`next.expandScan` resumes instead of repeating:** a `structureSearch`
  (`tree`, `files`) listing cut at `maxEntries`, or an `astSearch` (`match`,
  `symbols`) scan cut at `maxFiles`, offers `expandScan` on its last page with
  a doubled bound and the new continuation-only `scanOffset` (the rows or
  files already listed). The widened scan lists only what comes after them,
  and `astSearch` no longer re-parses those files. Earlier, the widened scan
  restarted at page 1 and listed every earlier row again. Following pages and
  `expandScan` to the end now reaches every entry, match, and declaration
  exactly once.
- **`astTopology` `next.expandScan` restarts explicitly:** graph rows are
  whole-graph answers (more files can retract a dead-code candidate or merge
  cycles), so a widened scan replaces the narrower one instead of resuming it.
  `expandScan` now rides only the last result page (not every page, and not
  diagnostic pages) and sets the new continuation-only `supersedes` (the cut
  scan's `maxFiles`); the widened page 1 echoes `supersedes` with a warning to
  discard the earlier rows. Earlier, every page offered the re-run and its
  rows looked like more of the same walk, so a walker could count them twice.
- **`astTopology` cuts that more files cannot lift are terminal:** a graph
  stopped at the 2,000,000-edge cap no longer offers `expandScan` (and no
  longer reports the cut as `maxFiles`); it reports `partialReasons:["edgeCap"]`,
  `terminalLimit:true` and a warning naming the cap. A `drift` cut at
  `maxFiles` now offers `expandScan` with `supersedes`; a `drift` that skipped
  files, hit the edge cap or the page ceiling sets `terminalLimit:true`
  instead of looking complete.
- **clasify `fileChunks` reaches every hit line:** hit lines a `localSearch`
  page only counted (`moreLinesUnlisted`) are listed by a one-file search
  before windows are planned, so they get a window (judged, or unjudged with
  its read) instead of dropping out of a walk that ends terminal. A search that
  cannot list them yields a `classificationBudgetSpent` page with the count and
  that search as `hints.read`. Measured repro: 242 of 296 hit lines covered.
- **clasify snippet reads cover the judged rows:** a `candidateEvidence:"search"`
  page's `hints.read` covered the densest hit cluster, which could hold none of
  the rows the page judged (0.96 on lines 107–108, read 489–609). It now
  covers every shown row: the densest window when it holds them all, else one
  window per cluster of shown rows.
- **clasify unjudged windows batched and stated once:** the hit windows a page
  or byte budget leaves unjudged share one `classificationBudgetSpent` page per
  file whose `hints.read` names each window once; in default output an error
  that pages with reads repeat is stated once on the resource (measured: 64
  identical error objects, 12.2 KB of 38 KB).
- **clasify judges each hydrated window once:** re-cut overlapping windows no
  longer judge or list the same window twice (9 duplicates in 86 pages).
- **Package registries honour a `Retry-After` HTTP-date:** `artifactSearch`
  now waits for (or, past its retry cap, returns) a registry's
  `Retry-After: <HTTP-date>` instead of retrying on its own backoff; it shares
  the one parser clasify uses.
- **One extension rule for local tools:** astSearch/astRewrite grammar
  selection, the lspSearch server probe, localSearch skipped-binary groups and
  private-key path detection use the engine's extension helper, so a dotfile
  such as `.pem` and the engine's own file walk agree.
- **A rejected stored GitHub token falls back to `gh`:** after GitHub answers
  401 for a stored or `gh` token, a running MCP server or runtime no longer
  selects that token again; the next request falls back (stored → `gh` →
  anonymous) instead of repeating "GitHub authentication required".
- **OAuth expiry with a negative UTC offset:** a stored `expiresAt` such as
  `…-05:00` is no longer read as already expired; login uses the same RFC 3339
  parser as the GitHub date handling.
- **Language-server memory cap on Linux:** the RSS watchdog now also runs on
  Linux, so a server tree whose summed resident memory passes `maxMemoryMb` is
  killed with "language server exceeded memory cap" (`RLIMIT_AS` bounds only
  each process).
- **Language servers that reserve large address space start on Linux:** the
  4 GiB `RLIMIT_AS` cap is gone where the RSS watchdog runs (macOS, Linux). It
  limited reserved, not used, memory, so a JVM server such as jdtls (default
  max heap a quarter of RAM), V8 WebAssembly cages or Go arenas could fail to
  start on large hosts. `maxMemoryMb` still bounds the resident memory of the
  whole server tree; other Unix targets, which have no watchdog, keep
  `RLIMIT_AS`.
- **`localFetch` regex reads no longer stop silently at 10,000 matches:**
  `regex:"rust"`/`"pcre2"` reads record up to 100,000 matches; a scan that
  finds more is `isPartial` with a warning naming the stop line, and
  `hints.textSearch` hands the same pattern and file to `localSearch`, which
  pages every match.
- **Content that contains the renderer's row placeholder renders verbatim:**
  text such as `__octocode_flow_row_0__` in a matched line is no longer
  replaced by another row; the renderer picks a placeholder absent from the
  response.
- **A second Ctrl-C ends a CLI call** whose work does not stop on the first
  cancel (exit 130).
- **Responses past 8 MiB page instead of failing:** they are served as row
  pages, or structured windows for one huge row, with an executable
  continuation.
- **Language-server teardown cannot signal a recycled pid:** the process-group
  sweep now runs while the exited leader is still unreaped (`waitid` with
  `WNOWAIT`), so its pid, which is the group id, cannot belong to an unrelated
  process yet. The leader is reaped after the sweep.
- **Extensions match case-insensitively everywhere:** search-hit kind labels
  (`declaration`, `comment`, …) now cover `.TS`-style uppercase extensions, and
  every engine extension check reads the file name, never a dotted directory.
- **`localSearch` exclude globs written as `!glob`** exclude like `glob`, as in
  `astSearch` (they were double-negated).
- **Path-policy error codes are the shared ones on every tool.** `ghCloneRepo`
  reported `ignoredPath` / `notFound` / `inputTooLarge` (outside the contract's
  `errorCode` list) where the local tools report `pathPolicyDenied` /
  `pathNotFound` / `fileTooLarge`, so those errors missed the shared recovery
  hints; `structureSearch` reported `structure.policy.<code>` where the others
  report `pathValidationFailed`.
- **`astRewrite` `include` reads like every other local tool's:** a bare word
  or a plain path (`api`, `src/api`) also selects the files under that
  directory, so an `astSearch` and the rewrite that follows see the same files.
- **`astTopology drift` summary** drops the always-empty `completeness.scopesAdded`,
  `completeness.scopesRemoved`, and the always-zero `metrics.observations`.
- **`skill check --fix` reports a failed repair** with its error and exits 1;
  it no longer ignores the installer result.
- **`@octocodeai/octocode-native` CommonJS entry exports `NATIVE_ABI_VERSION`**
  (it was `undefined`); `runtime.cjs` holds the one JS copy.
- **`skill` env readiness asks the native runtime** for a GitHub token stored
  outside the environment instead of probing `credentials.json` and `gh`
  `hosts.yml` itself.
- **CLI terminal errors keep their repair details.** An invalid tool input on a
  terminal now prints the same details as the JSON payload (for example
  "did you mean 'matchString'?"); a timeout's retry advice moved from `hints`
  to `details` in the JSON error.
- **`octocode install` enforces "never `octo mcp`".** The guard now runs on the
  install path, not only in tests.
- **One GitHub credential host.** CLI auth commands and tool requests derive
  the credential host from `github.apiUrl` through one runtime function, and
  stored-token refresh uses the same OAuth client-id fallback as `auth login`.
- **Sandbox.** The default allowed root is the workspace (`WORKSPACE_ROOT` /
  `local.workspaceRoot`, else cwd) plus `ALLOWED_PATHS` and `OCTOCODE_HOME`,
  never all of `$HOME`. Home-only keys (`OCTOCODE_BETA`, `ALLOWED_PATHS`,
  `WORKSPACE_ROOT`, `GITHUB_API_URL`, storage mode, …) are ignored in a
  workspace `.env` or `.octocoderc`.
- **Secret redaction** covers PEM blocks split across read windows and never
  lets `matchString` match a secret; `--redact-emails` masks emails in GitHub
  output.
- **`local*` tools: clearer path-denial errors ([#450]).** A path outside the
  allowed roots reports `Path '…' is outside allowed directories (allowed: …)`.
  The `Symlink target …` wording is reserved for genuine symlink escapes.
- **`.octocoderc` `local.allowedPaths` now takes effect** (home config only),
  adding roots like the `ALLOWED_PATHS` env var.
- **In progress:** relative paths resolve against `WORKSPACE_ROOT`;
  `invertMatch` with `resultView:"files"` lists files without the pattern; PR
  `matchString` implies patches and lists unsearched patchless files;
  `ghStructure` errors on a missing branch; compare pages carry only their own
  data and resolve `head` to a SHA; `artifactSearch` reports a missing package
  as `notFound`; nested TypeScript symbols are listed; `astTopology` never
  claims completeness with unresolved imports.
- **Docs** corrected against live behavior: allowed roots, the default
  `minify` (`none`), the 28-extension grammar registry (CUDA is opt-in), the
  file-and-listing-only GitHub cache, the 16 KiB read budget, `hover.range`
  (0-based), `OCTOCODE_BETA` gating `astTopology`, and the publish order.

### Notes
- The path allow-list remains **on by default** and cannot be disabled — only
  widened (`ALLOWED_PATHS` env or `local.allowedPaths`) or removed entirely with
  the whole local surface via `ENABLE_LOCAL=false`.

[#450]: https://github.com/bgauryy/octocode/issues/450
