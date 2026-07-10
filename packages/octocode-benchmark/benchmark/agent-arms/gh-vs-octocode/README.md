# gh vs Octocode CLI — GitHub Research Benchmark

Head-to-head on **pure GitHub research** (no local questions): code search,
exact file reads, repo discovery, structure browsing, PR/commit archaeology,
cross-repo comparison, defaults hunting, releases.

## Arms

| Arm | Whitelist | Notes |
|---|---|---|
| `gh` | `gh` CLI only (`gh search`, `gh api`, `gh pr`, …) | The GitHub-native baseline every agent already has. |
| `octocode` | `node packages/octocode/out/octocode.js` only | search/OQL, raw tools (`tools <name> --queries`), clone/cache. |

Both arms authenticate through the same GitHub token chain. Trimming pipes
(`| head` etc. inside `sh -c`) allowed for both. 3 solver agents per arm,
max 8 logged steps per question, every step wrapped by `../run-step.mjs`.

## Files

- `questions.md` — 10 solver-facing questions (no answers).
- `ground-truth.json` — judge-only verified answers + 0/0.5/1 rubric.
- Methodology, fairness rules, judging, and presentation format:
  [`../README.md`](../README.md). Solver prompt: [`../prompt-template.md`](../prompt-template.md).

## Replicate

1. `BENCH_OUT=packages/octocode-benchmark/output/gh-vs-octocode-$(date -u +%Y%m%dT%H%M%SZ)`;
   create `$BENCH_OUT/agents/{gh-1..3,octocode-1..3}` + `schemes/`.
2. Capture schemes (`gh --version`, `octocode --help`, `search --scheme`) into `schemes/`.
3. Spawn 6 solver agents from `../prompt-template.md` with the arm whitelists above.
4. Judge `answers.md` vs `ground-truth.json` → `$BENCH_OUT/scores.json`.
5. `node ../aggregate.mjs $BENCH_OUT` → tables for `results.md`.
