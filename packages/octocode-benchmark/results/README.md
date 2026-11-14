# Results index

The version timeline of Octocode's benchmark and the index of every stored round. This file, the
[per-question baselines](baselines/) and each round's `rounds/<version>-<date>/SCORECARD.md` are tracked;
all other data under `results/` and `campaigns/` is local and gitignored. Keep every folder as it is: these
are what new versions are compared against ([METHOD.md](../METHOD.md#compare-a-new-version)). Folder names
are relative to `results/` unless they start with `campaigns/` or `.octocode/` (the repository root's skill
output). The previous generated index is kept in `README-index-2026-10.md`.

## Version timeline

Newest first. Each new round adds a row: version, date, binary, scorecard, and its headline against the
previous version and the stored baselines.

| Version | Date | Build | Round (scorecard or stored folder) | Headline |
|---|---|---|---|---|
| 20.2.0 | 2026-11-14 | host release, SHA-256 `0dd29415…0d2d`, commit `73cddf5b32` plus shared dirty tree | [advanced capability scorecard](rounds/20.2.0-2026-11-14-advanced/SCORECARD.md) | Fresh rg/ast-grep/gh-lean/Graphify/direct-LSP comparisons: selected local/semantic evidence agrees; expert outputs smaller; graph queries faster in sample; explicit gh cache reverses the tested hybrid speed advantage; annotated-tag object gap |
| 20.2.0 | 2026-10-29 | native CLI, binary SHA-256 `30c301e0…a073`, commit `cb94ad744` plus uncommitted tree | `github30-v20-2026-10-29` | GitHub 30: correctness 9.57; 15,590 characters per question; 1.43× leaner than v18; gh-lean 0.47× v20's characters at 8.83 correctness; warm 1.5–2.3× faster than v18, cold 1.3–2.0× slower |
| 20.2.0 pre-release (`local-mcp-v9`, native 20.2.0) | 2026-10-10 | local MCP build | `local-30q-2026-10-10` | local core 30: correctness 10.0 vs lean shell 9.60; shell used 0.81× the characters; 19.1.0 used 2.32× |
| 20.x pre-release (`mcp-v9`) | 2026-10-10 | local MCP build | `lean-vs-mcp-30q-2026-10-10` | GitHub 30: correctness parity with gh-lean (8.83 vs 8.87); gh-lean leaner |
| 19.1.0 (published) | 2026-10-10/12 | `npx` | `lean-vs-mcp-30q-2026-10-10`, `local-30q-2026-10-10`, `pi-published-161933-2026-10-12` | the comparison point for the pre-release v20 builds |
| 18.2.2 (published) | 2026-08-08 | `npx octocode@18.2.2` | `full-octocode-vs-rtk-011533-2026-08-08`, `full-octocode-vs-headroom-091618-2026-08-08`; speed baseline in `github30-v20-2026-10-29/timing` | about 2.4× leaner than gh + RTK and 2.3× leaner than gh + Headroom at correctness parity |
| 18.1.1 (local build) | 2026-08-07 | `out/octocode.js` | `overview`, `full-octocode-vs-*-2026-08-07` | GitHub 30 × 3 passes: correctness 9.26; 22,111 characters per question; plain gh 2.0×, gh + Headroom 2.6×, gh + RTK 3.2× more characters |

Tool renames between v18 and v20 (needed to compare per tool across versions) are in
[baselines/release-notes-v18-v20.json](baselines/release-notes-v18-v20.json).

## Per-question baselines (tracked)

| File | What it holds |
|---|---|
| [baselines/github30.md](baselines/github30.md) | per question: characters and correctness for v20, gh-lean, v18, plain gh, gh + RTK, gh + Headroom |
| [baselines/local-core30.md](baselines/local-core30.md) | per question: characters, calls, seconds and correctness for the pre-release v20 build, its clasify variant, 19.1.0 and the lean shell |
| [baselines/release-notes-v18-v20.json](baselines/release-notes-v18-v20.json) | v18 → v20 tool map: renames and what changed per tool |

The August 2026 write-ups (v18.1.1 and 18.2.2 against gh, RTK and Headroom: `SUMMARY.md`, each campaign's
report, `per_question_summary.json`, the visual `index.html`) are also tracked on the `main` branch:
`git show main:packages/octocode-benchmark/results/<file>`.

## Rounds (tracked)

`rounds/<version>-<date>/SCORECARD.md`, one per round, from the [scorecard template](../briefs/SCORECARD.md).
- [20.2.0 — advanced comparisons, 2026-11-14](rounds/20.2.0-2026-11-14-advanced/SCORECARD.md):
  capability and workflow checks against fresh expert tools, with complete pages, cache arms,
  source witnesses and shared-host timing limitations.

## GitHub research, 30 questions ([questions/github](../questions/github/README.md))

| Folder | Date | Arms | What it holds |
|---|---|---|---|
| `github30-v20-2026-10-29` | 2026-10-29 | Octocode v20 CLI, gh-lean (stored answers) | The v20 headline: answers, per-call logs (`metrics.json`), blind packet, Opus 5.5 verdicts with swapped re-judges, v18-vs-v20 timing, `per-question.md`, frozen `harness/` (historical, not runnable) |
| `lean-vs-mcp-114934-2026-10-09` (raw: `campaigns/lean-vs-mcp-114934-2026-10-09`) | 2026-10-09 | gh-lean, MCP v3/v4/v9/v10, extras, published 19.1.0 | **The stored gh-lean baseline answers** that new Octocode rounds are judged against |
| `lean-vs-mcp-30q-2026-10-10` | 2026-10-10 | MCP v9, gh-lean, 19.1.0 | correctness parity with gh-lean; gh-lean leaner |
| `output-quality-2026-10-16`, `release-review-rebuilt-2026-10-15`, `release-fixes-2026-10-16` | 2026-10-15/16 | MCP builds vs gh-lean | release checks against lean gh |
| `pi-local-163239-2026-10-12`, `pi-published-161933-2026-10-12`, `pi-local-vs-published-164221-2026-10-12`, `pi-local-now-vs-published-231129-2026-10-12` | 2026-10-12 | local build, published 19.1.0, gh-lean (pi runner) | build against published release |
| `local-3arm-q1-10-2026-10-09` | 2026-10-09 | gh, gh-lean, MCP v5–v8 | Q1–Q10 iteration runs |
| `overview` | 2026-08 | Octocode v18.1.1, gh, gh + RTK, gh + Headroom (3 passes) | **The August baselines as numbers**: `per_question_summary.json`, `matrix_summary.json`, `MATRIX.md`, static dashboard `index.html` |
| `full-octocode-vs-gh-152630-2026-08-07`, `full-octocode-vs-rtk-162848-2026-08-07`, `full-octocode-vs-rtk-011533-2026-08-08`, `full-octocode-vs-headroom-134213-2026-08-07`, `full-octocode-vs-headroom-091618-2026-08-08` | 2026-08-07/08 | Octocode (local or v18.2.2) vs gh, RTK, Headroom | full-campaign reports; raw answers are not kept |
| `campaign-175653-2026-08-06`, `campaign-2pass-183432-2026-08-06`, `live-3arm-130222-2026-08-09`, `octocode-vs-gh-rtk-*-2026-08-0*`, `octocode-vs-gh-headroom-*-2026-08-0*` | 2026-08-05/09 | Octocode vs RTK, Headroom | earlier 20- and 25-question runs |
| `raw-ab-2026-08` | 2026-08-05/06 | Octocode vs RTK, Headroom | raw A/B runs, judge files and logs from that period (`runs/`, `logs/`) |

## Local code, pinned clones

| Folder | Date | Set | Arms | What it holds |
|---|---|---|---|---|
| `local-30q-2026-10-10` | 2026-10-10 | local-core | local MCP v9/v10, clasify v9, 19.1.0, lean shell | the local headline: Octocode 10.0 vs shell 9.60 correctness; shell 0.81× characters |
| `local-native-terra-2026-10-17` (raw: `campaigns/terra-cli-ab-20261017T094616Z`) | 2026-10-17 | local-scenarios | native CLI vs shell | exploratory, not one fresh agent per question |
| `local40-terra-goals-2026-10-13`, `LOCAL_MATRIX`, `local-goals`, `local-overview` | 2026-10-13 | local-goals | local MCP ± clasify | goals ablation |
| `local-ranking-2026-10-13`, `read-tool-ranking-2026-10-13` | 2026-10-13 | local | read and ranking variants | context savings by ranking |

## Local tools against rg and ast-grep (repository root `.octocode/octocode-eval-benchmark/`)

| Folder | Date | What it holds |
|---|---|---|
| `.octocode/octocode-eval-benchmark/2026-10-20-release-localsearch-vs-rg` | 2026-10-20 | localSearch vs rg 15.2.0: all 21 lexical cases agree; rg faster |
| `.octocode/octocode-eval-benchmark/2026-10-15-native-vs-rg` | 2026-10-15 | all local tools vs rg, development build |
| `.octocode/octocode-eval-benchmark/2026-10-20-release-astsearch-vs-ast-grep` | 2026-10-20 | astSearch vs ast-grep 0.51.0: all 41 shared and 17 edge cases agree |
| `.octocode/octocode-eval-benchmark/2026-10-19-owned-core-vs-ast-grep` | 2026-10-19 | owned structural matcher vs ast-grep |
| `.octocode/octocode-eval-benchmark/2026-10-15-native-vs-ast-grep` | 2026-10-15 | AST and LSP vs ast-grep |

## Classification (clasify, JEV)

| Folder | Date | What it holds |
|---|---|---|
| `mcp-clasify-ab-111154-2026-10-09` | 2026-10-09 | MCP with clasify on vs off, GitHub |
| `gated-jev-2026-10-13`, `gated-jev-github30-2026-10-13`, `gated-jev-local40-2026-10-13`, `gated-jev-github`, `gated-jev-local`, `gated-jev-*-datasets`, `gated-jev-accounting-2026-10-13`, `gated-jev-evidence-2026-10-13`, `GATED_JEV_GITHUB_MATRIX`, `GATED_JEV_LOCAL_MATRIX` | 2026-10-13 | gated clasify enabled vs disabled, 30 GitHub and 40 local questions |
| `terra-goals-2026-10-13`, `github30-terra-goals-2026-10-13` | 2026-10-13 | goals with and without JEV |
| `selective-guidance-2026-10-13`, `selective-guidance-{github30,local40,transfer12}*`, `SELECTIVE_GUIDANCE_*_MATRIX`, `selective-semantics-2026-10-13`, `review-classification-2026-10-13` | 2026-10-13 | clasify guidance variants |
| `beta-jev-all-tools-20261017T131700Z` | 2026-10-17 | beta and JEV all-tool recheck |

## Retired lanes (kept, no longer run)

Local models through FreeLlama or Ollama with the native tools. These compare models, not
Octocode with a competitor, and are no longer part of the benchmark.

| Folder | Date |
|---|---|
| `local-model-all-tools-20261017T1520`, `local-model-search-20261017T143311Z`, `local-model-tools-pilot-20261017T1510`, `local-model-issues-and-options-20261017`, `jev-local-model-pilot-20261017`, `mac32-model-selection-20261017`, `devstral-native-pilot-20261017`, `bonsai-native-ollama0406-20261017`, `gemma-local-instructions-20261017T142630Z`, `gemma-without-freellama-20261017T142950Z`, `gemma-without-freellama-20261017T142958Z`, `freellama-mcp-smoke-20261017T141737Z`, `ollama-native-smoke-20261017T141117Z` | 2026-10-17 |

## Other

| Folder | What it holds |
|---|---|
| `archive-unified-2026-09` | September–October 2026 unified-results archive (dedup, production review, `s12`, `ff-final2`, `p7-final` runs) |
| `runs` | per-run records from the old summary scripts |
| `campaigns/*` | raw transcripts, answers, receipts and judge attempts for the October campaigns; JSON inside holds absolute paths, so the folders stay where they are |
