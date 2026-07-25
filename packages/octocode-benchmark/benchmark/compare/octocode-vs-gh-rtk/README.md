# Octocode vs (rtk + gh) — GitHub Research-Flow Benchmark

10 GitHub research questions, each a different research-flow shape, solved by
independent subagents. 100% remote, 9 repos, 3 languages (TS/JS, C, Go). Q6 is
open-ended.

- **Arm A (`rtk-gh`)**: `rtk` (`grep/read/ls/tree/find/gh/git/json/wget`) + `gh` CLI.
- **Arm B (`octocode`)**: ONLY `node packages/octocode/out/octocode.js`.

| Q | Flow category | Repo(s) |
|---|---|---|
| Q1 | Cross-repo comparison | `pmndrs/zustand` + `vercel/next.js` |
| Q2 | How-it-works / flow trace | `vercel/next.js` |
| Q3 | Deep/large PR review | `vuejs/core` (#15035) |
| Q4 | Bug/issue validation (RCA) | `pmndrs/zustand` |
| Q5 | Find-in-large-repo | `microsoft/vscode` |
| Q6 | Exploratory cross-repo comparison | `vuejs/core` + `sveltejs/svelte` |
| Q7 | Deep dive / architecture exploration | `nodejs/node` |
| Q8 | npm package → source-repo research | `esbuild` → `evanw/esbuild` |
| Q9 | How-it-works / flow trace #2 | `fastify/fastify` |
| Q10 | Root-cause analysis #2 (security bug) | `redis/redis` (#15389/#15433) |

Each question stresses a different tool surface so no single command sequence
solves all ten: cross-repo comparison, flow tracing, deep multi-file PR review,
verify-against-current-source bug validation (two languages), large-repo
navigation, open-ended comparison, architecture deep-diving, and package→source
resolution with a verify-don't-assume trap.

## Known non-determinism

Q3 (#15035) and Q10 (#15389/#15433) are merged/closed — stable. Q4's "still
open" verdict (PR #3531) and its exact regex line ARE time-sensitive —
re-verify before trusting an old snapshot and record the date. Q8's
Go-vs-JS/`child_process` facts are architectural and stable; only
download-count/version figures drift.

Shared method + metrics: [`../README.md`](../README.md). Ground truth was
verified independently via WebFetch (not either arm) — see
`ground-truth.json`'s `verificationCaveat`.
