# Changelog

## [Unreleased]

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

### Changed
- **Warm `lspSearch` across CLI calls.** The first call of a workspace starts a
  private per-workspace server (`<home>/run`, owner-only socket, 10-minute idle
  exit); later calls skip the language server cold start (TS hover ~3 s → ~0.1 s).
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

### Fixed
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
