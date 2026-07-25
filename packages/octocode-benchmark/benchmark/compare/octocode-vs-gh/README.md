# Octocode vs `gh` CLI — GitHub Task Benchmark

10 GitHub tasks, each chosen to expose a **specific `gh` CLI limitation** and
test whether Octocode does better. Same LLM, same tasks, same budget.

- **Arm A (`gh`)**: ONLY the `gh` CLI (`gh search code`, `gh api`, `gh pr view`,
  `gh search prs`, …) plus reading the JSON it returns.
- **Arm B (`octocode`)**: ONLY `node packages/octocode/out/octocode.js`.

| Q | Stresses (gh weakness) | Repo |
|---|---|---|
| Q1 | Text search false positives — AST needed | `facebook/react` |
| Q2 | Token cost of reading one function from a big file (minify) | `microsoft/TypeScript` |
| Q3 | Deep PR review — file/delta breakdown | `expressjs/express` |
| Q4 | Navigate a large monorepo to one file | `vercel/next.js` |
| Q5 | Which PR/commit introduced a symbol (history) | `pallets/flask` |
| Q6 | Absence trap — honest "not defined" | `sindresorhus/is` |
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
independently verified**. Before a scored run, verify each `answer` with a method
outside both arms (WebFetch `raw.githubusercontent.com` / `api.github.com`),
freeze it, and record the date. Q6 (absence) and the capability shape of each
question are stable regardless.

Shared method + metrics: [`../README.md`](../README.md).
