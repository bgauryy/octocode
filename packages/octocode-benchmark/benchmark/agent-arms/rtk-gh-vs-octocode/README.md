# rtk+gh vs Octocode CLI — Agent Research Benchmark

Compares two agent toolchains on 10 root-cause / research-flow questions
(5 local on this repo, 5 remote GitHub/npm), solved by independent subagents.

- **Arm A (`rtk-gh`)**: `rtk` (token-optimizing CLI proxy: `rtk grep/read/ls/tree/find/gh/git/json/wget`) + `gh` CLI.
- **Arm B (`octocode`)**: ONLY `node packages/octocode/out/octocode.js` (search/OQL, tools, clone/cache).

3 solver agents per arm; every research command is wrapped by the shared
`../run-step.mjs <agentOutDir> <stepId> -- <cmd...>`, which logs
`{id, cmd, exit, ms, bytes}` to `commands.ndjson` (bytes of stdout+stderr =
token-cost proxy) and saves raw output.

Files:
- `questions.md` — solver-facing question set (no answers). Q10 was replaced
  after run `20260710T182339Z` with a parity-safe remote→local question (the
  original npm one needed a borrowed `npm view` in Arm A — retired).
- `ground-truth.json` — judge-only verified answers + scoring rubric.
- Shared harness (`run-step.mjs`, `aggregate.mjs`, `prompt-template.md`) and
  methodology/fairness rules: [`../README.md`](../README.md).

Metrics per question/agent: correctness (0/0.5/1 vs rubric), steps, bytes
consumed, wall ms. Aggregated per arm in the run's `results.md`.

Output layout follows `recipes/agent-benchmark-runbook.md`:
`output/rtk-gh-vs-octocode-<TS>/` with `agents/<arm>-<n>/{answers.md,commands.ndjson,raw/}`
plus README/manifest/summary/results/reflection/ratings at the run root.
