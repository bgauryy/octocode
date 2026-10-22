# Changelog

## [Unreleased]

### Removed — astTopology folded into graph query (2026-10-22)
- **The beta `astTopology` tool is gone** from the CLI, `schema`, and the tool contract; the catalog has 14 tools. Run `octocode graph ingest <dir>` (an unchanged tree reuses its snapshot), then `graph query`.
- Replacements: `deadCode` → `issues --detector unreachable-file,unused-export`; `reachability` → `issues --detector unreachable-file,test-only`; `cycles` → `cycles` (witness and cuts: `issues --detector cycle`); `dependencies`/`dependents` → `deps`/`dependents <file> [--depth n]`; `path` → `path <from> <to>`; `drift` → `drift --graph <head> --baseline <baseline>`.
- **New in `graph`:** `query drift --baseline` (rows `{category: file|cycle|transition|relation, change}`), `issues --entry <ref,...>` for explicit entrypoints, `dependents` rows through re-exporting modules (`edge: uses, via: reexport`), `graph.partialReasons` with a `graph.reingest` command on answers over a cut scan, and `ingest --language-glob <lang>=<glob>` (repeatable) for root-relative parser choice.
- Old installs keep an unused `<octocode home>/tmp/topology/` directory; delete it by hand.
- **The keychain no longer reads a bare token.** The `octocode` keychain entry holds only the JSON credential record. No released build ever wrote a bare token there: releases up to 19.1.0 kept credentials in the encrypted `credentials.json`, which is still read.

### Fixed — native audit (2026-10-21/22)
- **`graph ingest` redacts secrets before writing a snapshot.** A credential the graph keeps (an import specifier stored as a package name, a diagnostic quoting source) is stored as its redaction placeholder, e.g. `pkg:[REDACTED-GITHUBTOKENS]`.
- **The dotenv secret pattern no longer redacts code** such as `frame.expect_key = frame.object;`.
- On a paged batch, a rejected row's `hints` are a string list, not an internal object.
- `responsePageMismatch` and `invalidClassificationView` are declared error codes; a restart past the last page answers `hasMore: false`.
- ghSearchHistory `updated:30d` (and other relative dates) matches again.
- localSearch `modifiedWithin` too large to represent is `invalidInput` instead of a silently dropped filter.
- `minify: "standard"` no longer halves indentation twice.
- `graph` edges carry their source line, and `pub use` re-exports link calls, so `callers` of a re-exported Rust function resolve.
- A selected read shows one `filtered` notice, not one per stage.
- artifactSearch release tags are checked as tag refs: a branch with the same name no longer counts.
- ghGetHistoryItem commit pages read by branch continue at the pinned commit SHA.
- GitHub pacing is chosen by request route, so GraphQL and REST calls are spaced by their own limits.

### Changed — native audit (2026-10-21/22)
- **structureSearch: `next.expandScan` is now `next.continueWalk`.** A listing cut at `maxEntries` widens through `continueWalk` (doubled `maxEntries`, `scanOffset`), the same lead that resumes a time-cut walk (`walkOffset`); astSearch keeps `expandScan`. A filtered `tree` offers a read lead, an empty listing diagnoses its filters in one probe walk, and the walk reads sibling directories ahead (1.7× faster on a 1.08M-entry tree).
- **localFetch pages fill their window**: about 89–93% of `responseLength` instead of about 64%, with each row cut once from the view it holds; continuations keep the page size asked for.
- **localSearch prices pages lazily**: a cold first page for `self` went from 237 ms to 86 ms, and continuation pages read no files. Earlier cursors restart (snapshot v8).
- ghCloneRepo's `location.verified` is gone; a clone cached by an older build is cloned again.
- Stored formats changed and rebuild on first use: GitHub disk cache v4 (raw bytes beside a header line; old `.json` entries are pruned), graph facts schema 2, `graph.bin` v4 and its memo.
- A large GitHub file read (the graphql-spec repo case) takes 0.70 s instead of 3.36 s.
- Development: `cargo dev-test` runs the workspace tests with the right features; a nightly CI job runs the full test matrices; the generated `tool_types.rs` holds only the types native names (6.1 MB → 0.86 MB, runtime check 11.8 s → 3.3 s); the runtime crate's modules import only lower layers, guarded by a test.

### Changed — cleanup and speed (2026-10-22)
- **`graph query dependents` names imports that cannot resolve**: an import that names a file by path (`vs/base/common/x`) with no tsconfig or workspace link is listed under `summary.unlinkedPathImports`, and `diagnostics` rows of code `unlinked-path-import` carry its `target`. Snapshots are format v5; older ones ask for a re-ingest.
- **`graph ingest` is faster**: a cold vscode ingest takes 6.8 s instead of 17.3 s (next.js 11.2 s instead of 18.4 s); snapshots and answers are byte-identical.
- **Wide searches are faster**: localSearch over 13k hits answers in 95 ms instead of 223 ms on a warm server, and astSearch `match` in 83 ms instead of 169 ms; output is byte-identical.
- **Validating rejected input or output is about 9× faster** per case (median 15.5 ms → 1.7 ms).
- **`octocode --help` and `auth --help` start about 40% faster**: help without a tool section no longer parses the tool presentation.
- Outlines and graph facts find doc comments by the grammar's comment style: `.pyi` stubs read `#` comments, assembly reads `;`.
- Development: the generated wire types moved to their own crate (`crates/contract-types`); a runtime edit re-checks in 2.1 s instead of 3.6 s and rebuilds in 4.3 s instead of 6.2 s.
- **Every CLI command starts faster**: start-up no longer parses the 340 KB CLI presentation or compiles the secret scrubber's patterns at run time (`--version` 9.0 → 6.9 ms, an unknown command's JSON error 22.6 → 12.1 ms).
- **Secret redaction catches Unicode case variants** of case-insensitive keys (`elevenlabſ_api_key`, the Kelvin sign), which the scrubber's prefilter skipped before.
- `graph query issues` scans mentions without a lock per file: up to 2× faster under load, same output.
- lspSearch: CUDA rows get the `noCompileDatabase` warning like C and C++; a `file://` URI with percent-escapes is decoded before prewarming.
- astSearch only sends declared error codes (an unreadable file is `fileReadFailed`, not `structural.file.unreadable`).
- A localSearch query rebuilt by regex repair no longer carries the old saved selection plan.
- A command's output that was already written when it exited is read, not lost, even when its grace period is zero.

### Changed — CLI install, cache command, error wording (2026-10-21)
- **`install` manages every scope a client reads.** `--scope home|workspace|local` writes the user config, the project file under the current directory (Cursor, Claude Code, Continue, Kiro, Codex) or Claude Code's per-directory section, resolved like the config view. `--remove` deletes the entry with a rolling backup (no entry is a no-op; an entry install did not write needs `--force`). `--list` and the terminal picker show each client's status (`installed`, `disabled`, `custom`, `absent`, `invalid`, `unsupported`) and config path; `--list --json` rows are objects. `--pass-env` says it stores the token in the client config in plain text.
- **Removed the hidden `cache status|clear` command.** Delete `tmp/response/`, `tmp/artifact/` or a `tmp/clone/` checkout under `octocode config home` instead; `logs/evictions.jsonl` still records every clone eviction.
- **One wording for an unknown field.** `Remove unknown field 'x' from queries[0] (did you mean 'y'?)` whether or not other issues come with it, and the repair-lead note names row fields the same way (`queries[0].x`).
- **artifactSearch crates dependencies keep their target.** A crate that declares a dependency per platform lists each as `name req; cfg(...)` instead of two identical entries.
- `octocode --help` states that a batch exits with its worst row while stdout carries every row.

### Fixed — astSearch walk ceiling, astTopology partial graphs (2026-10-21)
- **astSearch `match` and `symbols` past the 100,000-file walk ceiling name what they never read.** The terminal row keeps `terminalLimit` and adds one `narrowScope`, `narrowScope2`, … hint per unread directory (and one for a directory's own unread files), so the calls together reach every unread file; scopes past eight are counted and listed through a `structureSearch` lead. A single directory above the ceiling is a typed `flatDirectory` limit. Queries with positional globs get no scopes and a warning saying why.
- **astTopology with `maxFiles` at its maximum is never `empty` over a truncated graph**, and `scopeTooBroad` offers one `narrowScope` hint per admissible directory it names (up to five).
- localFetch documents its 1 GiB ceiling as terminal (`fileTooLarge`) with the alternative outside Octocode.
### Fixed — agents-communication: Python-parity fixes from the port review (2026-10-20)
- **A hook whose stdout reader closed early no longer loses mail.** `hook` and
  `host-hook` post-tool delivery record the offer `uncertain` (needs an explicit
  `retry_delivery`), not `submitted`, and exit 0 like the Python runtime did.
- **A UTF-8 byte order mark stays content.** Re-sharing an identical document that starts with
  U+FEFF succeeds, and paged `read_document` reassembles it byte for byte; hook stdin may
  begin with a mark.
- **A backslash in a lease path is a filename character on POSIX**, so `a\b` no longer
  conflicts with a tree lease on `a`.
- **Blank text follows Python `str.strip()` and `\s`**: U+0085 and U+001C–U+001F are blank,
  U+FEFF is not (message text, `reasoning`, `--session`, `--vendor`, record search words).
- **MCP:** an integer request id beyond 2^53 is refused (`-32600`) instead of being echoed as a
  different id, and `tools/call` with `arguments: null` is a tool error. A hook
  `context_generation` that is not a string is refused. Queued lease targets are stored
  ASCII-escaped again, so a retry finds its wait.
- `leave` always appends `coordinate.out`, also for an expired or repeated leave, so the owner-lock reclaim never renews a left session.
- An unreaped (zombie) owner process counts as dead in the delivery-owner liveness probe, and `health` probes each owner once per call.
### Fixed — tool audit: catalog notation, streamed reads, scan pacing (2026-10-20)
- **`octocode schema` catalog fields read as the input you type.** A tool with
  several row forms lists each as one flat row with its selector inline,
  `[operation:"files", path*, …] | …`, instead of variant labels such as
  `tree[…]` or `npm:exact[…]` that read as nesting. Core
  `publishedFormFields` marks the selector fields (the discriminator, or a
  required field whose values differ between forms, like artifactSearch
  `ecosystem`) with their values.
- **A streamed multi-range localFetch no longer discards the ranges that fit.**
  On a source over 10 MiB, a read whose range needs several window pages
  serves every range before it plus that range's first page, continues it
  through `next.continueWindow` and reads the ranges after it through
  `next.readOtherRanges`; every requested line is on exactly one page.
- **Large-source memos hold a working set.** The streamed line index and
  range-view memos are bounded by their byte weight (8 MB, 4 MB); their entry
  caps rose from 16 to 1024 and 256, so alternating among more than 16 large
  files no longer re-hashes each one.
- **Timed scans pace their chunks.** astSearch and astTopology scans size each
  chunk from measured throughput (a tenth of the 10 s budget, never past
  what is left) instead of fixed 512-file chunks, so slow storage overshoots
  the budget by about one short chunk.

### Changed — agents-communication live harness and JSON output
- `src/live.mjs`: the `--skill` flag is now `--package`.
- JSON output (CLI and MCP tool results) refuses NaN and Infinity, which are not valid JSON; they fail instead of being written.

### Removed — agents-communication no longer needs Python (breaking, 2026-10-20)
- **`@octocodeai/octocode-agents-communication` runs on Node.js only.** The
  Python runtime, its launcher scripts, the portable runtime archive
  (`pack:runtime`) and `OCTOCODE_PYTHON` are gone, with no compatibility
  layer. The runtime is TypeScript bundled into `dist/` and uses the built-in
  `node:sqlite` (Node.js `^24.15.0`, SQLite with FTS5). Existing databases
  open unchanged: application ID, schema version and fingerprint are the same.
  The package version bump is left to the release owner.
- **Upgrade rule:** run one runtime per database. The delivery-owner lock
  moved from a file lock to a Unix-domain socket (Windows: named pipe,
  untested), so an old and a new `listen`, `dispatch` or `run` do not
  exclude each other; stop old owners first.
- **Delivery-owner liveness is the owner process (R18).** A frozen owner no
  longer reads dead after about 128 probes: the old probe connected to the
  owner's socket and filled its accept backlog. The owner now records its pid,
  start time and socket inode in a sidecar `<socket>.pid`, and a probe is
  `kill(pid, 0)` plus those reuse guards; `health` uses the same probe.
- **A slept or stalled worker keeps its identity (R14).** While its process holds
  the delivery-owner lock, an expired presence row is renewed by the next
  heartbeat with the unexpired leases no other owner took meanwhile, instead of
  failing until `resume`; staging waits for the renewal rather than throwing.
  Heartbeats are due when either the monotonic or the wall clock has advanced a
  period, so a wake is noticed at once. Without the lock, expiry still means the
  owner died. A worker killed between spawning the vendor and spawning its
  watchdog still leaks the vendor (R15, documented in `SERVICE_PROTOCOL.md`).
- **Behavior and limits that changed:** `activity` `pathRegex` is a JavaScript
  `RegExp` with the `u` flag (ASCII-only `\d \w \s \b`; no `(?P<name>)`,
  `\A`, `\Z` or inline flags); whole-number floats such as `5.0` are
  integers; a database path over 504 bytes after resolving symlinks is refused
  up front with its length and the limit (R17; SQLite opens 504 bytes and
  fails at 505 on Node 24.15.0 and 26.4.0); a killed managed worker's vendor process group is stopped by a
  detached watchdog on POSIX, while Windows has no Job Object and leaks the
  vendor tree of a worker killed outright; lone UTF-16 surrogates in input are
  refused.
- **Protocol fixes:** a `checkback` must name a pending `task` you received
  (F1); a `checkBack:true` task refuses `done`, `partial` and `failed`
  until a check-back is sent, `refused` and `canceled` exempt (F2); RACI
  informed peers get an answer body that leads `Re <kind> <id>: <headline>`
  (F4); empty lists in message slots are refused (F5); a malformed
  `document:` evidence pin is refused instead of stored as text (F6). Docs
  now route "needs input" through `blocker`, not `delta` (F8).
- **Catalog and signals:** slot titles render from the catalog instead of
  hard-coded labels, so `task.checkBack`, `decision.default`, `informed` and the
  `issue` slots read the same on every host (F3); `handoff` takes an optional
  `constraints` list and `delta` an optional `phase`, rendered in protocol order
  (F7); the catalog's `messageKinds` markers (request vs FYI, wake) replace the
  hard-coded request and wake sets, checked against the `schema.sql` kind CHECK
  (F11); SIGINT and SIGTERM end one-shot commands such as `inbox wait` with
  130/143 because loops now register themselves (`listen`, `mcp`, `view` and
  `run` exit 0; an interrupted `dispatch` with a delivery in flight exits 1)
  (R19).
- **Docs and skill:** the package docs moved from `scripts/docs/` to `docs/`;
  the public `octocode-agents-communication` skill is a lean lobby that links the
  operating guide returned by `skill --json` instead of restating it, and
  fixes its `db info`, `listen`, custom-database and `fetch --incoming`
  statements.

### Changed — localSearch `matchString` list; one hidden-folder flag (2026-10-18)
- **`localSearch` `matchString` takes a list**, as `localFetch` does: up to 10
  texts, any of which a line may hold. Each entry is literal unless `regex` is
  set, and `caseMode:"smart"` judges each on its own. A list runs as the one
  regex alternation that finds the same lines (`next.*` leads carry it).
- **`includeHidden` alone reaches tool-config dot directories** (`.github`,
  `.vscode`, `.config`, `.agents`, …) in `localSearch`, as in `structureSearch`;
  text search no longer prunes them by name. A dot directory a `.gitignore`
  rule names (`.octocode/`) also needs `includeIgnored`; the result discloses
  it and its `includeSkipped` lead carries the flag. Generated dot directories
  (`.cache`, `.venv`, …) and credential stores stay excluded.

### Removed — internal limits are no longer settings (2026-10-15)
- **Timeouts, retries, waits, page size, caches, concurrency and selection
  budgets are code constants**, not `.env` or `.octocoderc` settings:
  request timeout 30 s, 3 retries, 45 s GitHub rate-limit wait, a
  20,000-character automatic page (`responseLength` per call still goes to
  50,000), a 24 h / 2 GiB / 50-repository clone cache, 10 classification
  requests in flight, and 64 candidates / 131,072 characters / 10 s per
  context selection. The language-server memory cap is host-derived; a
  per-server `maxMemoryMb` in `lsp-servers.json` still overrides it.
- **Removed names** (`network.timeout`, `network.maxRetries`,
  `network.maxRateLimitWait`, `lsp.maxMemoryMb`, `output.pagination`,
  `storage.cloneCache`, `classification.maxConcurrency`, `.maxCandidates`,
  `.maxChars`, `.timeoutMs`, and `REQUEST_TIMEOUT`, `MAX_RETRIES`,
  `OCTOCODE_MAX_RATE_LIMIT_WAIT`, `OCTOCODE_LSP_MAX_MEMORY_MB`,
  `OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH`, `OCTOCODE_CACHE_TTL_MS`,
  `OCTOCODE_MAX_CACHE_SIZE`, `OCTOCODE_MAX_CLONES`,
  `OCTOCODE_CLASSIFICATION_CONCURRENCY`, `_MAX_CANDIDATES`, `_MAX_CHARS`,
  `_TIMEOUT_MS`) get one `removed_setting` warning each and are ignored;
  startup never fails on them. `config set` refuses them; `config unset`
  still deletes them.

### Fixed — security, lossless reads and leads (2026-10-15)
- **`lspSearch` importer recovery no longer drops the gitignored-files
  scan silently.** The scan for ignored files that spell the name has a 3 s
  bound; on a repository with large ignored checkouts it stopped there and
  returned nothing, so the row read as having no ignored importer. The row
  is now partial (`importerScanIgnored`) with a warning that the scan did
  not finish, the files it reached, and a `next.includeSkipped`
  `localSearch` over the whole scope.
- **Clone cache checks access on every hit.** A cached `ghCloneRepo`
  checkout is served only after one `commits/{ref}` call with the caller's
  own credential succeeds; any failure refuses the hit, so one token can no
  longer reuse another's private checkout. A host serving several users
  should still give each its own `OCTOCODE_HOME`.
- **`fullContent` pages are lossless.** A `ghGetFileContent` (or
  `localFetch`) `fullContent` read too large for one page continues raw
  (no `minify`) unless you chose a view; the pages join to the whole file.
- **`ghGetHistoryItem` PR misses return leads.** A `matchString` that hits
  no patch line offers `withoutMatchString`, then `readFiles`; an
  `include`/`status`/`minChanges` scope that matches nothing offers
  `readFiles`. The prose tips are gone.
- **`astSearch` `match` takes `languageGlobs`** and warns with a count and
  example paths when a directory scan skips files no grammar parses.
- **`localSearch` pages hold more rows with `enclosing` names:** each row is
  charged its exact name cost instead of a flat reserve (about 1.6× the rows
  on a single large file); pages stay identical across calls, and an edit to
  an earlier page's file restarts the walk with `staleSnapshot`.
- **Language servers:** a missing built-in server's error names its install
  command; a `tsgo` `workspaceSymbol` answer at its 256-symbol limit warns
  that the server capped it.
- **Validation:** a `clasify` input that misses a branch's required selector
  now reports that branch's error, with selector issues first.
- **`config view`:** an edit conflict and a contract mismatch (`UNAVAILABLE`)
  exit 5; invalid input stays exit 2.

### Changed — configuration (2026-10-14)
- **A workspace may narrow, never widen, home-only switches:**
  `OCTOCODE_ENABLE_LOCAL=false`, `OCTOCODE_REDACT_EMAILS=true` and
  `OCTOCODE_STORAGE_MODE=memory` apply from a workspace `.env` or
  `.octocoderc`; the widening value is skipped there.
- **Path settings expand `~`** (`local.allowedPaths`, `local.workspaceRoot`,
  `lsp.configPath`, `lsp.cacheDir`) once, when settings resolve; a relative
  path is rejected with a warning.
- **New warnings:** a tool list naming no tool (`unknown_tool_name`, with the
  nearest name) and a removed setting (`removed_setting`: `version`,
  `classification.type`, `OCTOCODE_CLASSIFICATION_TYPE`).
- **`config get` masks secrets** and says where the value lives (home,
  workspace or process environment); `--json` adds `source` and `path`.
- **`astRewrite` is parked:** unavailable on MCP and the CLI (`disabled`),
  with or without beta.
- **`clasify` is beta:** it needs `OCTOCODE_BETA=true` and a classification
  key. The key alone enables `clasifyContext` on `localSearch`, `localFetch`,
  `ghSearchCode` and `ghGetFileContent`, which keeps only the hits or passages
  a judge selects; the earlier ranking, `locate`, Scout and `next.clasify`
  walk are removed.

### Changed — GitHub reads and leads (2026-10-14)
- **Identical concurrent GitHub file reads send one request**, and a cached
  file at a resolved commit is served without a conditional request.
- **A terminal page keeps the default lead set.** A page that ends at a
  terminal limit no longer lists every lead with every row; `extras:["hints"]`
  shows the rest, as on any complete page.


### Fixed — tool-review round (2026-10-12)
- **GitHub code search never queues a call it will then refuse.** A wait the
  rate-limit window already puts past the cap is refused before queueing, and
  a row with several `extensions` is refused before its first search when
  they cannot all start in time (searches the cache answers need no slot).
  A client-side refusal no longer reports a made-up `rateLimit.remaining: 0`,
  so anonymous callers are no longer told to log in for it.
- **`ghSearchRepo` warns when GitHub ignored `language`.** GitHub drops a
  language it does not know and returns unfiltered rows; a page with no row in
  the requested language says so when its rows mix languages, or when they
  share one and one more search without `language` counts the same (an alias
  GitHub resolved, like `js`, counts fewer and gets no warning).
- **`lspSearch` names the files its heritage scan could not read**
  (`partialReasons: heritageUnread`, plus a text-search lead over them); it
  used to give a bare count.
- **The warm CLI `lspSearch` server works with any home.** Its socket moved
  from `<home>/run` to `<temp dir>/octocode-<uid>`, so a long home (a macOS
  `$TMPDIR` one) no longer skips it silently (every call started cold, ~2.6 s);
  an unusable socket location is stated on stderr.
- **Hints fit 120 characters as sent.** The hint check now reads whole source
  files (a one-line test-module item ended its scan early), and the shortened
  unknown-login and empty-structural-match hints fit.
- **The instructions name the `queries[]` envelope** (agents left it out in
  8 of 111 benchmark calls).

### Changed — CLI paging is success; exit 6 removed
- **A result with a next page exits 0**, as the MCP server reports it:
  `pagination.hasMore`, `next.*` and a partial source read no longer change
  the exit code. `graph query` cuts also exit 0.
- **Exit 6 is gone.** Nothing else used it. A retryable failure (`next.retry`)
  beside answering rows now exits with its failed row's class (`7` rate limited,
  else `5`). Every other code is unchanged: empty `1`, invalid input `2`
  (including a malformed `snapshot`), not found `3`, auth `4`, execution `5`,
  rate limited `7`, interrupted `130`.

### Fixed — astSearch and lspSearch paging and language servers
- **Pages fill a byte budget.** `astSearch` and `lspSearch` fill one ~16 KiB
  page by bytes; a count (`pageSize`, `matchPageSize`) is an explicit override
  only. A 3000-match file walks in 14 calls (was 30 at 100 rows), 1538
  references in 18 calls (was ~3 KB pages), every item once with one total.
- **`astSearch` scans the whole scope in one call** within a time budget and
  resumes exactly once past it (`scanOffset`); a huge-ts symbol search is one
  complete call instead of an exit-6 `maxFiles` continuation.
- **A server that hits its memory cap is a final `serverCrashed`** with an
  unverified text lead, not a retryable crash. The default cap is 25% of host
  RAM clamped to 4-8 GiB; per-server `maxMemoryMb`/`env` live in
  `lsp-servers.json`; TypeScript gets `maxTsServerMemory`; a workspace `tsgo`
  serves `.ts/.tsx/.mts/.cts` (with `GOMEMLIMIT` = cap - 512 MiB); the pool
  evicts idle servers once their summed resident memory passes the cap.
- **`supertypes`/`subtypes` work without a type hierarchy.** Subtypes are the
  server's implementation answer (not the declaration itself); supertypes are
  the heritage clause's definitions; the row says which.
- **No false importer partial for a vendored copy.** A candidate that declares
  its own same-named symbol is rejected, not left "unverified".
- **A push-timeout `diagnostic` is partial** (`diagnosticsNotPublished`), not
  "no diagnostics".
- A single `astSearch` symbols page states `pagination.hasMore`.

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
- **Moved under `storage`:** `session.enableStats` → `storage.stats` (env name
  unchanged).

### Breaking — `@octocodeai/config` package
- **Public API trimmed to what Octocode uses:** `CONFIG_FIELDS`, `DEFAULT_CONFIG`,
  `ENV_TOKEN_VARS`, `INTERACTIVE_EXECUTION_TIMEOUT_SECS`, `RuntimeSurface`,
  `applyOctocodeEnv`, `configFieldEnvName`, `contractDriftAllowed`,
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
  private per-workspace server (owner-only socket under
  `<temp dir>/octocode-<uid>`, 10-minute idle exit); later calls skip the
  language server cold start (TS hover ~3 s → ~0.1 s).
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
  `next.*` continuations, warnings, and errors; a row's `extras` (`hints`,
  `details`, `stats`, `receipts`, `echo`, `diagnostics`) opts into the rest.
- **Leaner text output.** A single-row YAML response drops the
  `results`/`index`/`data` wrapper, path-only search rows render as compact
  strings (`path` or `path (count)`), and `localFetch` numbers lines with an
  `rg -n` style gutter (`279:`). JSON and `structuredContent` keep the envelope.
- **`defaultExcludes`** on `localSearch`, `structureSearch`, and `astSearch`
  controls the built-in dependency/build-output excludes. Sensitive paths stay
  denied and `.gitignore` still applies without `includeIgnored`.
- **PR review.** `ghGetHistoryItem` pull requests list changed files as compact
  rows (`"M +3 -1 [!flag ]path[ <- old]"`) grouped by directory; `include`,
  `status` and `minChanges` narrow files and patches; `matchString` returns
  only matching hunks with `contextLines` and a `next.readFullPatches` step;
  patch reads carry a slim identity header.
- **clasify** answers come from a judgment cache with in-flight dedupe.
- **Ranking.** Identifier searches rank the declaring file first.
- **MCP instructions** stay within 2,000 characters; the grammar inventory is
  CLI-only. `structureSearch` defaults to `operation:"tree"`.

### Removed
- **`octocode-clasify` skill merged into `octocode-research`.** Remove old
  installs with `octocode skill remove octocode-clasify`.
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
- **Page totals:** a compare's commit `pagination` states `totalItems` (GitHub's
  `totalCommits`) and `totalPages`; every byte page states
  `pagination.totalBytes`.
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
- **Entry scripts run through a symlink:** the local-testing harness, the
  `skills-dev/octocode-dev` scripts, the agents-communication maintainer
  scripts and the benchmark helpers compare
  real paths to detect being run directly, so a symlinked path (macOS `/tmp`
  included) no longer exits silently without doing anything.
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
  lets `matchString` match a secret; `OCTOCODE_REDACT_EMAILS=true` masks
  emails in GitHub output.
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

### Notes
- The path allow-list remains **on by default** and cannot be disabled — only
  widened (`ALLOWED_PATHS` env or `local.allowedPaths`) or removed entirely with
  the whole local surface via `OCTOCODE_ENABLE_LOCAL=false`.

[#450]: https://github.com/bgauryy/octocode/issues/450
