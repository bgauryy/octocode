# rtk+gh vs Octocode CLI — GitHub Research-Flow Benchmark

Compares two agent toolchains on 10 GitHub research questions (**v2**), each a
different research-flow shape, solved by independent subagents. 100% remote,
9 repos, 3 languages (TS/JS, C, Go), no `facebook/react`. Q6 is open-ended.

| Q | Flow category | Repo(s) |
|---|---|---|
| Q1 | Cross-repo comparison | `pmndrs/zustand` + `vercel/next.js` |
| Q2 | How-it-works / flow trace | `vercel/next.js` |
| Q3 | Deep/large PR review | `vuejs/core` |
| Q4 | Bug/issue validation (RCA-style) | `pmndrs/zustand` |
| Q5 | Find-in-large-repo | `microsoft/vscode` |
| Q6 | Exploratory cross-repo comparison | `vuejs/core` + `sveltejs/svelte` |
| Q7 | Deep dive / architecture exploration | `nodejs/node` |
| Q8 | npm package → source-repo research | `esbuild` → `evanw/esbuild` |
| Q9 | How-it-works / flow trace #2 | `fastify/fastify` |
| Q10 | Root-cause analysis #2 (security bug) | `redis/redis` |

- **Arm A (`rtk-gh`)**: `rtk` (`grep/read/ls/tree/find/gh/git/json/wget`) + `gh` CLI.
- **Arm B (`octocode`)**: ONLY `node packages/octocode/out/octocode.js`.

3 solvers/arm; every command wrapped by `../run-step.mjs`, logging
`{id, cmd, exit, ms, bytes, tokens, tokenizer}` to `commands.ndjson` (tokens =
primary cost metric).

Files:
- `RESULTS.md` — latest run summary. **Currently v1 (6Q)** — not comparable to a v2 run; regenerate via `node ../aggregate.mjs <v2-runDir>` once one exists.
- `questions.md` — solver-facing, v2 (10 questions). Frozen once any solver is dispatched.
- `ground-truth.json` — judge-only answers + rubric, v2.
- Shared harness/methodology: [`../README.md`](../README.md).

## v2 (2026-07-12)

Replaced v1 Q3 (a docs-only PR, too shallow) with a real multi-file
cross-package `vuejs/core` PR review (#15035). Added Q7-Q10 for deep-dive,
npm-package research, a second flow trace, and a second RCA question, in new
repos/languages. Q1/Q2/Q4/Q5/Q6 carried over unchanged (already verified).
Ground truth for the new/replaced questions was verified independently via
WebFetch (`raw.githubusercontent.com`/`api.github.com`/`registry.npmjs.org`),
not either arm — see `ground-truth.json`'s `verificationCaveat` for the one
remaining gap (itemized per-file PR diff stats sourced from `gh`, cross-checked
against WebFetch-fetched aggregates).

## Why these flow categories

Beyond plain symbol-finding, this set isolates: cross-repo comparison (Q1),
flow tracing (Q2, Q9), deep multi-file/cross-package PR review (Q3),
verify-against-current-source bug validation (Q4, Q10 — two languages so no
toolchain can coast on one ecosystem), large-repo navigation (Q5), open-ended
comparison (Q6), architecture deep-diving past a thin public API (Q7), and
package→source resolution with a verify-don't-assume trap (Q8). Each stresses
a different tool surface so no single question can hide a capability gap.

## Run sizing recommendation (next v2 run)

Keep `agentsPerArm = 3` for the first run (v1's N=3 already had power to
surface a real, reproducible cross-arm failure on Q5). Raise step budget
8 → ~10-12: Q3 and Q7 have 3-4 sub-parts each across multiple files/packages
and would hit an 8-step ceiling on research, not on capability. Apply the
judge discipline in `../README.md` step 8 (read the actual answer, not just
keyword-match; independently re-verify disputed cells; prefer blinded scoring).

## Known non-determinism

Q3, Q4, Q10 anchor to real PR/issue numbers. Q3 (#15035) and Q10 (#15389/#15433)
are merged/closed — stable. Q4's "still open" verdict (PR #3531) and its exact
regex line ARE time-sensitive — re-verify before trusting an old snapshot and
record the date in `manifest.json`. Q8's Go-vs-JS/`child_process` facts are
architectural and stable; only download-count/version figures will drift.
