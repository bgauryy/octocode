# Octocode vs `gh` CLI — GitHub Task Benchmark

10 GitHub tasks, each chosen to expose a **specific `gh` CLI limitation** and
test whether Octocode does better. Same LLM, same tasks, same budget.

- **Arm A (`gh`)**: ONLY the `gh` CLI (`gh search code`, `gh api`, `gh pr view`,
  `gh search prs`, …) plus reading the JSON it returns.
- **Arm B (`octocode`)**: ONLY `node packages/octocode/out/octocode.js`.

| Q | Stresses (gh weakness) | Repo |
|---|---|---|
| Q1 | Text search false positives — AST needed | `vuejs/core` |
| Q2 | Token cost of reading one function from a big file (minify) | `microsoft/TypeScript` |
| Q3 | Deep PR review — files/delta + diff-grounded root cause | `expressjs/express` |
| Q4 | Cross-repo dependency hop (router moved out of the repo) | `expressjs/express` → `pillarjs/router` |
| Q5 | Symbol history + deep commit-history **pagination** | `pallets/flask` |
| Q6 | **Repo search** discovery + absence trap — honest "not defined" | `sindresorhus/is` |
| Q7 | npm package → source repo → real language | `axios/axios` |
| Q8 | Find every caller of a function (LSP vs grep) | `lodash/lodash` |
| Q9 | Outline a large file cheaply (symbols mode) | `redis/redis` |
| Q10 | Many searches under the 10 req/min code-search limit (caching) | `nodejs/node` |

## Why these

`gh` CLI is fast and cheap for one-off ops but has **no AST search, no LSP, no
local code intelligence, no output minification, and no result caching** — it is
`grep` over GitHub's text index plus raw JSON dumps. Q1/Q8 need structural or
semantic precision text search can't give; Q2/Q9 punish raw whole-file/whole-JSON
reads on token cost; Q6 is an absence trap; Q10 stresses the shared 10 req/min
code-search rate limit that Octocode mitigates with session caching + local
routing.

## Oracle status

⚠️ **Q1–Q10 oracles are DRAFT** — the answer text is architecturally plausible
but line numbers, exact counts, and PR states are **time-sensitive and NOT yet
independently verified**. Per the shared validity gate this suite is
**orientation only, not a ship-gate** until a one-time verification pass freezes
every oracle (WebFetch outside both arms). Q6 (absence) and each question's
capability shape are stable regardless.

**Contamination:** these are famous repos — run the **no-tools control arm** (see
shared README) first; any question the control already answers measures LLM
memory, not tools, and must be down-weighted or replaced. Each question's
`capabilityPoint` drives the **tool-use grading layer** (did Arm B use AST / LSP
/ minify / cache; did Arm A hit the rate limit).

## Tool/feature coverage map (enforced by `expectedWorkflow` in ground truth)

| Feature | Questions |
|---|---|
| Code search with filters (`match:file`/`match:path`, repo/path scoping) | Q1, Q4, Q5, Q10 |
| **Repo search** (`ghSearchRepos`, stars-sorted discovery) | Q6 |
| Repo structure (`ghViewRepoStructure`, incl. file sizes) | Q4, Q6 |
| Targeted fetch (`ghGetFileContent` **matchString** slice) | Q2, Q4, Q7 |
| Minified/outline read (`minify:"symbols"`) | Q9 |
| **Pagination** (history beyond page 1) | Q5 |
| PR search + deep review (`ghSearchPullRequests` reviewMode) | Q3 |
| Commit history (`ghSearchCommits`, path-filtered) | Q5 |
| npm → source repo (`npmSearch`) | Q4, Q7 |
| Clone + AST / LSP (`ghCloneRepo`, `localSearchCode`, `lspGetSemantics`) | Q1, Q8 |
| Batching + session cache under rate limit | Q10 |

Each ground-truth question now carries an `expectedWorkflow` — the trajectory
layer grades Arm B's logged calls against it (substance, not exact order).

Shared method + metrics (three arms, trajectory grading, aggregation, validity
gates): [`../README.md`](../README.md).
