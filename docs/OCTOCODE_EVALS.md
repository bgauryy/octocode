# Octocode Evals

Release-readiness benchmark for Octocode CLI commands, raw MCP tools, feature
checks, and cross-tool flows. Scores rate eval coverage, not product quality.

> `octocode search` (OQL) has its own questions-and-tool eval:
> [`docs/octocode-language/OCTOCODE_SEARCH_EVAL.md`](octocode-language/OCTOCODE_SEARCH_EVAL.md)
> — 45 research questions, each with the exact `octocode search` invocation,
> across all 11 active targets plus routing/continuation/materialization checks.

Scale: `10` full (dispatch + execute + envelope + behavior + pagination +
negative + cross-flow) · `8-9` core + one of pagination/negative/cross-flow,
env-gated gap · `6-7` smoke only (side-effectful/interactive) · `<6` not
release-ready.

## Coverage Summary — `9.1/10`

| Feature Area | Rating |
|---|---:|
| CLI discovery/help/context | 10 |
| Raw tool schema discipline | 10 |
| Bulk raw queries | 10 |
| Local structure/find/read | 10 |
| Text search | 10 |
| Structural search | 9 |
| GitHub code/repo/content/tree | 9 |
| PR/history archaeology | 8.5 |
| npm/package lookup | 8 |
| LSP semantics | 8 |
| Pagination | 9.5 |
| Minification/content views | 9 |
| Binary inspect/strings | 9 |
| Archive list/extract/decompress | 9.5 |
| Clone/cache remote-as-local | 9 |
| Output contract | 9.5 |
| Error/gate behavior | 8.5 |
| Management commands | 7 |
| Adding CLI commands | 8 |
| Current language support | 9 |
| Expanding language support | 8.5 |

## CLI Commands

| Surface | Rating |
|---|---:|
| global help | 10 |
| context | 10 |
| tools list | 10 |
| tools --scheme | 10 |
| tools <name> --queries | 10 |
| tools bulk queries | 10 |
| ls local | 10 |
| ls symbols | 9 |
| ls remote | 9 |
| cat local exact | 10 |
| cat minification | 9 |
| cat local slice | 10 |
| cat pagination | 9 |
| cat remote | 9 |
| grep text | 10 |
| grep regex/options | 9 |
| grep pagination | 9 |
| grep structural | 9 |
| grep remote text | 8.5 |
| grep --repo | 9 |
| find local | 10 |
| find remote | 8.5 |
| diff | 9 |
| lsp | 8 |
| repo | 8.5 |
| pr list | 8.5 |
| pr detail | 8 |
| history | 8.5 |
| pkg | 8 |
| binary inspect | 9 |
| binary strings | 9 |
| binary list | 10 |
| binary extract | 9.5 |
| binary decompress | 9.5 |
| unzip | 9.5 |
| clone | 9 |
| cache fetch | 9.5 |
| cache status | 9 |
| cache clear | 7 |
| status | 8 |
| install | 6.5 |
| login | 6 |
| logout | 6.5 |

## Raw Tools

| Tool | Rating |
|---|---:|
| ghSearchCode | 9 |
| ghSearchRepos | 8.5 |
| ghHistoryResearch | 8.5 |
| ghGetFileContent | 9.5 |
| ghViewRepoStructure | 9 |
| ghCloneRepo | 9 |
| localSearchCode | 10 |
| localFindFiles | 10 |
| localGetFileContent | 10 |
| localViewStructure | 10 |
| localBinaryInspect | 9.5 |
| lspGetSemantics | 8 |
| npmSearch | 8 |

## Feature Checks

| Check | Rating |
|---|---:|
| Bulk query payloads | 10 |
| Pagination | 9.5 |
| Minification/content views | 9 |
| Binary & archive handling | 9.5 |
| Output contract | 9.5 |
| Current language matrix | 9 |
| Language expansion path | 8.5 |
| Command expansion path | 8 |

## Bulk Queries

Rating: `10/10`.

Raw `tools <name> --queries` supports all documented input shapes:

- single object: `{"path":"...","keywords":"..."}`
- array: `[{...},{...}]`
- envelope: `{"queries":[{...},{...}]}`

The schema hard-limits each raw tool call to `1-5` queries. A live six-query
probe returned `queries: Too big: expected array to have <=5 items`, which is
the desired failure. The eval should assert that a two-query call returns two
`structuredContent.results[]` entries with distinct auto-filled IDs, and that a
six-query call exits as bad input.

## Extensibility

Command expansion rating: `8/10`.

Adding a quick command is available, but not one-file automatic. The runtime
command must be added under `packages/octocode/src/cli/commands/`, registered in
`commands/index.ts`, and documented in external `@octocodeai/octocode-core/cli`.
`command-spec-coverage.test.ts` enforces that every registered command has a core
spec and that runtime options are documented there. This is good safety, with the
main friction that command behavior and command metadata currently span two repos.

Adding a raw tool is more automatic once the actual tool exists: schema and
description live in `octocode-core`, execution lives in `octocode-tools-core`,
the MCP server registers the thin surface, and the CLI `tools` command discovers
the shared catalog. Quick commands are optional convenience wrappers on top.

Language expansion rating: `8.5/10`.

Current language support is strong: the generated support matrix reports `143`
known extensions, `38` structural AST extensions, `25` signature outlines, `33`
LSP server mappings, and `105` minify-only extensions. Full rich support for a
new language needs the Rust grammar/minifier/LSP path wired, not just a CLI flag:

- add or enable the tree-sitter grammar crate in `packages/octocode-engine`
- register extensions, language id, body query, and comment style in
  `src/signatures/languages.rs`
- ensure structural `langType` maps to extension globs in
  `packages/octocode-tools-core/src/tools/local_ripgrep/structuralSearch.ts`
- add or confirm LSP grammar/server mapping when semantic navigation is wanted
- add minification strategy or confirm minify-only fallback
- regenerate/verify `packages/octocode-benchmark/benchmark/SUPPORT.md`

Availability: once rebuilt, the language becomes available to the surfaces its
support level enables: text search/minification for minify-only, structural grep
for tree-sitter-backed languages, symbol outlines when a body query exists, and
LSP when a server mapping and installed server are available.

## Cross-Flows

| Flow | Rating |
|---|---:|
| local search → exact read | 10 |
| structural search → LSP | 8.5 |
| remote → local cache | 9.5 |
| archive → local tools | 9.5 |
| package → source repo | 8 |
| PR archaeology | 8 |
| binary extraction → content | 9 |

## Status Labels

`pass` · `pass-empty` · `blocked:auth` · `blocked:native` · `blocked:lsp` ·
`blocked:artifact` · `skip:side-effect` · `fail`

## Findings — 2026-06-21 live run

Verified all 42 CLI surfaces + 13 raw tools + 4 feature checks + 7 cross-flows
against a freshly built CLI (`packages/octocode/out/octocode.js`, rebuilt via
`yarn workspace octocode build:dev`), auth via stored `gh cli` credentials.
Overall **9.1/10** corroborated; ratings unchanged.

### Actionable

1. **`grep --only-matching --unique` renders a flat list** — run against
   `packages/octocode/src/cli/routing.ts`: returns 9 bare `resolveRef` strings
   with no file/line annotation and no visible dedup. Caps `grep regex/options`
   at 9. Add per-file grouping or count context.

### Resolved on rebuild

2. **`grep` positional-shape footgun** — previously
   `grep <keywords> <path> --pattern <shape>` silently treated the positional
   as a literal text keyword. On the fresh build the CLI now errors cleanly:
   `✗ Structural search (--pattern/--rule) takes a single local PATH as its only
   positional — it has no text keywords. You passed 2 … Try: grep <path>
   --pattern "<shape>"`. No longer a defect.

### Informational (confirm ratings)

3. **`lsp documentSymbols` auto-routed to `ls --symbols`** at the quick-command
   layer (`commands/...`) — intentional UX choice; raw
   `lspGetSemantics type:documentSymbols` still available. Keeps `lsp=8`.
4. **`ls`/`grep` on deep remote paths renamed on `main` returns "Not found"** —
   passes the empty-result contract cleanly per `AGENTS.md` (verify
   scope/branch before treating as absence). Local copy diverged from remote
   `bgauryy/octocode`, expected.

### Build note

`yarn build` fails at the `octocode` (CLI) package's `lint` step on **pre-existing**
prettier errors in `tests/cli/exit-codes.test.ts` and `tests/cli/github-error.test.ts`
(both untouched this session — timestamps predate the only session edit to
`docs/OCTOCODE_EVALS.md`). The `build` script runs `lint` before `node build.mjs`,
so the failure leaves `out/` stale. `yarn workspace octocode build:dev` (skips
lint) produces a working CLI. Fix: `yarn workspace octocode lint:fix` on those
two test files (out of scope for this eval run).
