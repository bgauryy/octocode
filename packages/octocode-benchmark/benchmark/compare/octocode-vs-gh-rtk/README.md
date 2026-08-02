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

This is the only **verified ship-gate** suite (oracles independently confirmed
via WebFetch, not either arm — see `ground-truth.json`'s `verificationCaveat`).
Still run the **no-tools control arm** first: several targets are famous repos, so
any question the control already answers measures LLM memory, not tools, and must
be down-weighted. The flow category per question maps to the capability the
**tool-use grading layer** should confirm Arm B exercised.

## Tool/feature coverage map (judge-side `expectedWorkflow` per question)

Questions are frozen (verified ship-gate); coverage is enforced through the
trajectory layer, which checks Arm B's logged calls against each ground-truth
question's `expectedWorkflow`:

| Feature | Questions |
|---|---|
| Filtered code search (repo/path scoping) | Q2, Q5, Q6, Q10 |
| Search **pagination / refinement** on a huge repo | Q5 |
| Repo structure (incl. **file sizes**) | Q1, Q6, Q7 |
| Targeted fetch (**matchString** slices, not whole files) | Q1, Q2, Q4, Q6, Q7, Q8, Q9, Q10 |
| Branch-pinned fetch (verify on `main`/`unstable`, not the PR) | Q4, Q10 |
| PR search + deep review (reviewMode:full) | Q3, Q4, Q10 |
| Issue search → PR → current source workflow | Q10 |
| npm → source repo + language metadata | Q8 |
| Docs + source cross-check | Q9 |

Shared method + metrics (three arms, trajectory grading, aggregation, validity
gates): [`../README.md`](../README.md).
