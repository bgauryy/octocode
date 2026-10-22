# octocode local-tool test bench

End-to-end checks for the local MCP tools (`structureSearch`, `localSearch`,
`localFetch`, `astSearch`, `lspSearch`) against the repo's **built** MCP server
(`packages/octocode-mcp/dist/index.js` + the native addons). Tracked: harness and fixtures. Not tracked: the cloned repos (see [repository inventory](../repos/README.md)), generated grammar and large fixtures, and raw run output.

```sh
node skills-dev/octocode-dev/scripts/dev.mjs build:dev                              # from the repo root, after changing native
node packages/octocode-benchmark/local/e2e/harness/run-all.mjs                 # hermetic quality suites
OCTOCODE_LIVE=1 node packages/octocode-benchmark/local/e2e/harness/run-all.mjs # + live suites (GitHub, registries, classifier)
node packages/octocode-benchmark/local/e2e/harness/run-all.mjs grammar,workflows   # only the named suites
node packages/octocode-benchmark/local/e2e/harness/debug-call.mjs localFetch '{"path":"/abs/file.ts"}'
```

`results/<suite>.log|json` holds each run; `results/summary.json` the totals. run-all prints one
`PASS <suite>` or `FAIL <suite>` line per suite and a final `PASS run-all` / `FAIL run-all` line,
and exits non-zero when any suite fails, errors, times out or reports no checks.

## Suites

| Kind | Suites | Runs |
|---|---|---|
| Hermetic (default) | `acceptance` (acceptance.test.mjs), `usage-regressions`, `lsp-importers`, `rewrite`, `grammar`, `workflows`, `chain-check`, `lead-replay`, `navigate`, `repo-sweep`, `deps-flows`, `large-files`, `local-ranking`, `read-tool-selection`, `classification-retry`, `tool-health` | Always. Only local clones and fixtures; run-all gives each a fresh empty `OCTOCODE_HOME` and `GH_CONFIG_DIR`, strips GitHub and classifier credentials, and routes HTTP(S) to a dead proxy (loopback exempt, so `tool-health`'s loopback mock works) |
| Live | `remote`, `github`, `review-large`, `artifacts`, `clasify` | Only with `OCTOCODE_LIVE=1`, with your own credentials. The same switch enables the GitHub/registry sections of `chain-check` and `lead-replay` |
| Bench (not correctness) | `perf`, `competitors`, `kpi`, `soak` | Only when named |

The hermetic environment comes from `test-utils/hermetic-env.ts`, the same definition the
MCP, CLI and config vitest suites run under (`test-utils/external-effects-guard.ts`).
Harness scripts also run when invoked through a symlink (`harness/entry-symlink.test.mjs`).

## Layout

| Path | What |
|---|---|
| `harness/mcp-client.mjs` | Shared stdio MCP client, `next.*` page walker, check recorder |
| `harness/workflows.mjs` | Regression checks for every fixed defect + continuation hygiene |
| `harness/local-ranking.mjs` | Local search/fetch `clasifyContext` selection edge cases with a loopback classifier (the all-tool contract is `read-tool-selection`): query-view schema gating, per-call provider failures, exact candidate/character budgets, matchString/range/CRLF anchors, streamed and oversized sources, plan binding (source path/bytes, settings, view), clasify source capture and its read lead, legacy-input rejection, redaction, stats, and CLI/MCP replay |
| `harness/read-tool-selection.mjs` | All-13 read-tool applicability plus context selection for `localSearch`, `localFetch`, `ghSearchCode`, and `ghGetFileContent`: selected-only CLI/MCP bodies, complete/partial/unavailable receipts, policy thresholds, paging, unfiltered recovery, optional metadata, long-context headers, sentinels, single-candidate judgment, capability gates, and no hidden hydration |
| `harness/classification-retry.mjs` | Hermetic CLI/MCP/N-API checks for exact judgment reuse, account isolation, retry limits and usage, failed flights, and source/classifier/account changes during selection-plan replay |
| `harness/usage-regressions.mjs` | Input shapes and failures mined from recorded agent sessions, replayed on MCP and the CLI: batch/brief placement, guessed field names, wrong-tool rows, regex repairs, bounded large reads, root-policy hints, warm LSP |
| `harness/grammar.mjs` | All 12 Tree-sitter grammars and their extensions on small fixtures |
| `harness/navigate.mjs` | Agent-style ORIENT → SEARCH → READ → PROVE walk per grammar repo, each hop fed only by copied values; hop/byte budget |
| `harness/tool-health.mjs` | Every contract tool over CLI and MCP on tiny fixtures and a loopback GitHub/registry/classifier mock (`harness/tool-health/`): happy path, full `next.*` walks, every lead explored by `tool-health/driver.mjs` (contract-valid, accepted, key-family predicates, per-call facts only), empty, invalid, CLI/MCP parity. Hermetic, about 15 s, `--deep` about 25 s (`node packages/octocode-benchmark/local/e2e/harness/tool-health.mjs [tool,...] [--deep]` or `dev.mjs test:tools`); results in `results/tool-health.json` |
| `harness/remote.mjs` | Live canary: GitHub search/read/history, artifactSearch, clasify against the real services; every returned `next.*` page and `hints.*` lead must execute (tool-health covers the same shapes against the mock) |
| `harness/github.mjs` | GitHub data fidelity against the local clones at the pinned SHA: file bytes/ranges/matchString/byte chunks, code-search precision + verbatim snippets + filters + pagination, tree = `git ls-tree`, commit/compare/PR = `git diff` (with patches, `include` selection and pageSize-1 patch paging), history filters, open-PR head SHAs, `clasifyContext` selection over reads and code search at the pinned SHA, clasify typed questions over a pinned GitHub source |
| `harness/review-large.mjs` | A >100-file PR: inventory pages, a late file's patch and its patch windows, against GitHub's REST files API |
| `harness/lsp-importers.mjs` | CommonJS importer recovery past unrelated comment mentions |
| `harness/chain-check.mjs` | Contract sensor: per-tool chain steps plus acceptance checks C1-C5 (`harness/acceptance.mjs`, unit-tested by `acceptance.test.mjs`) |
| `harness/lead-replay.mjs` | Replays every `next.*`/`hints.*` lead of a fixture set and checks it does what its key promises |
| `harness/artifacts.mjs` | artifactSearch fidelity: 8 registries (npm, PyPI, crates, Maven, NuGet, Go, Packagist, RubyGems) against each registry's own API; the `viewRepo` hint names and opens the same repo |
| `harness/perf.mjs` | Cache effectiveness: lspSearch page 2 vs page 1, symbols continuation pages and repeated analyses, tokio `graph` ingest, reuse and dependents |
| `harness/clasify.mjs` | Real-provider classification (runs with `OCTOCODE_BETA=true`): `clasifyContext` must select the target declaration in large unread files and the implementing file in localSearch, two classifiers over one file, a selected walk in source order with `next.unfiltered` recovering every line once, host bytes against a full read; clasify noul/choice/score over held state, a localFetch source and a URL capture with their `read` hints, and a failed source row |
| `harness/repo-sweep.mjs` | One big real repo per grammar: every tool, full pagination, cross-tool correlation |
| `harness/large-files.mjs` | Extremely large files (3 MB checker.ts, 28 MB log, 1-line 1.9 MB bundle, 20k-function file) with pagination walked to the end |
| `harness/competitors.mjs` | Frozen competitor tasks (`harness/competitor-tasks.json`, local corpora + pinned GitHub refs) run as an octocode recipe and as the expert shell recipe (rg/sed/git/gh/ast-grep/npm); same truth check both sides, calls/bytes/ms per side, ratios and medians in `results/competitors.json`. Normalized by default: local tasks run with the MCP workspace and shell cwd at the corpus repo root, short agent-sized briefs, and an `unfiltered` first-try shell arm reported beside the expert arm (`OCTOCODE_COMPETITOR_NORMALIZE=0` = legacy octocode-root run). Sensors: `nextShare`/`hintsShare`, optional calls per hint list (reported without a fixed cap), never-trim (a truncation signal needs a paging continuation or a terminal-limit disclosure). Fails on a wrong octocode answer (including a row seen twice or never across a row-continuation walk) or a never-trim violation in a fully walked recipe. Clasify tasks need `OCTOCODE_COMPETITOR_CLASIFY=1` (paid). S1 sensors and the byte gate: see [Competitor sensors](#competitor-sensors) |
| `harness/sensors.mjs` | Pure sensor functions for `competitors.mjs` (continuation pages vs leads, schema errors, verbose fields, body hash + key byte shares, byte gate); `node harness/sensors.mjs --self-test` |
| `harness/version-compare.mjs` | Rates every tool of the local build (`--side local`) or of a published release (`--side published --version <v>`, installed into a temp prefix) with the same steps from `harness/version-compare/cases.mjs`: success, characters read, latency, continuation completeness (the `next.*` walk against an unpaged call), empty and invalid-query guidance, and CLI/MCP parity. Not part of `run-all`: it takes a side, needs the network and a `gh auth token` for GitHub tools, and writes one JSON per side (`--out`, default `$TMPDIR/version-compare-<side>.json`); run it once per side and compare the two files. `--tools a,b` limits it |
| `harness/verbose-fields.json` | Configurable list of default-output fields (no `extras`) the verbose sensor reports |
| `fixtures/lsp/` | TS/JS layouts that hide importers from tsserver (inferred project, CommonJS alias) |
| `fixtures/grammar/` | Generated per-language fixtures (regenerated by `grammar.mjs`) |
| `fixtures/large/` | Generated large files (regenerated by `large-files.mjs`) |
| `repos/` | Shallow clones, one per grammar (see below) |

## Competitor sensors

`competitors.mjs` records these per task in `results/competitors.json` (`tasks[].octocode.sensors`, `overall`):

| Sensor | What it measures | Effect |
|---|---|---|
| `nextShare` / `hintsShare` / `leadShare` | Bytes under `next`, under `hints`, and of lead continuations wherever they live, over response bytes | Report |
| Lead entries per menu | Optional calls decoded from `hints` strings; historical streams also count named hint objects and non-page `next` entries. Required pages are separate | Report only; schema bounds still apply to each executable call |
| Schema errors | MCP/CLI input-validation errors, plus rows with a schema `errorCode` | Fails above 0 |
| Verbose fields | Default-output fields from `harness/verbose-fields.json` (for example `responsePagination` with `hasMore:false`, or pagination fields that a page continuation repeats). Rows queried with a non-empty `extras` are skipped | Report only |
| Verbatim replay | Each continuation in `next` and each non-`text` entry in `hints` runs unchanged. Entries that the recipe already ran count; up to `OCTOCODE_COMPETITOR_REPLAY_MAX` (default 3) others are called per task. CLI-only tools are counted, not run | Fails on a validation error |
| Byte gate | Each task is compared with the pinned anchor `results/competitors-anchor.json` and the rolling baseline `results/competitors-rolling.json`. Bytes or calls above 1.5× a baseline fail when the ratio to the shell also gets worse; if the shell recipe changed, the absolute growth alone fails. Local tasks also fail when unique evidence (file:line pairs, files) shrinks or never-trim violations grow. Each record stores a body hash and key byte shares, so a flag names the key that grew. A changed octocode recipe is reported and not compared | Fails |

Each page continuation is recognized by its name: `next*`, `continue*`, `restart`, `searchUnpatchedFile`, and `responsePagination.next`. The sensors read both continuation shapes: the current one, where `next` holds only pages and `hints` is `{text?, <lead>: {tool, query}}`, and the legacy one, where `next` also held leads and `hints` was a list of prose strings.

```sh
node packages/octocode-benchmark/local/e2e/harness/competitors.mjs --self-test          # sensor self-tests, no server
OCTOCODE_BETA=true node packages/octocode-benchmark/local/e2e/harness/competitors.mjs --update-rolling   # accepted run → rolling baseline
OCTOCODE_BETA=true node packages/octocode-benchmark/local/e2e/harness/competitors.mjs --update-anchor    # explicit re-anchor (only byte-gate checks may fail)
```

The rolling baseline is updated only when `--update-rolling` is passed and every check passed. The anchor changes only with `--update-anchor`. Both commands merge the tasks of the run into the existing file. The anchor was seeded on 2026-10-03 with `OCTOCODE_COMPETITOR_CLASIFY` unset, so the four clasify tasks have no anchor record.

## Repos

Clone list with pinned commits: [repository inventory](../repos/README.md). Shell baselines need `rg` on PATH (or `RG_BIN=/path/to/rg`).

Clone again with `git clone --depth 1 https://github.com/<repo>.git repos/<dir>`:

| dir | repo | grammar |
|---|---|---|
| typescript | microsoft/TypeScript | TypeScript (+ Go in the TS 7 port) |
| tsx | excalidraw/excalidraw | TSX |
| javascript | lodash/lodash | JavaScript |
| python | django/django | Python |
| go | prometheus/prometheus | Go |
| rust | tokio-rs/tokio | Rust |
| java | google/guava | Java |
| c | redis/redis | C |
| cpp | nlohmann/json | C++ |
| csharp | JamesNK/Newtonsoft.Json | C# |
| scala | typelevel/cats | Scala |
| asm | libjpeg-turbo/libjpeg-turbo | Assembly |

LSP rows need the language server on PATH (TypeScript/JavaScript, Rust, C/C++
are available here; Python, Go, Java, C#, Scala, Assembly report
`serverUnavailable`, which the suites record rather than fail on).
