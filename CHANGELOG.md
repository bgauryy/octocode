# Changelog

## [Unreleased]

### Changed
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
