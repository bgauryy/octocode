# Results — rtk+gh vs Octocode CLI

> **v1 (6-question) results only** — superseded by v2 (10Q, see `README.md`).
> Not comparable; regenerate via `node ../aggregate.mjs <v2-runDir>` once a v2 run exists.

Latest v1 run: [`rtk-gh-vs-octocode-flows-20260712T075326Z`](../../../output/rtk-gh-vs-octocode-flows-20260712T075326Z/)
— live run, `INTEGRITY OK`, scored by an independent judge subagent on
correctness + depth-of-quality with an explicit false-positive hunt. Full
detail in that run's `results.md`/`judge-notes.md`; this is the short version.

## Headline

| Arm | Mean correctness /6 | Mean quality /5 | Mean tokens (est.) | Mean KB | Mean tool time (s) | Tokens/correct |
|---|---:|---:|---:|---:|---:|---:|
| **octocode** | 5.67 (94.4%) | **4.44** | 23,639 | 63.2 | 67.2 | 4,172 |
| **rtk-gh** | 5.67 (94.4%) | 4.08 | **12,429** | **33.9** | **29.0** | **2,193** |

Tied on correctness (34/36 sub-questions right). rtk-gh is ~1.9x cheaper per
correct answer despite more steps (36.3 vs 27.0) — cheaper per-step. All 2
misses land on Q5 (3/6 agents named `_dispatch` instead of `dispatchEvent`),
not arm-specific.

## Per-question

| Q | Flow | Result |
|---|---|---|
| Q1 | Cross-repo (zustand + Next.js) | 6/6: 1.0 |
| Q2 | How-it-works (Next.js route regex) | 6/6: 1.0 |
| Q3 | PR review (zustand #3541) | 6/6: 1.0 |
| Q4 | Bug validation (zustand devtools, PR #3531) | 5/6: 1.0; octocode-3: 0.5 (missed PR#) |
| Q5 | Find-in-large-repo (vscode dispatch) | 3/6: 1.0; 3 confident-wrong FPs (judge-notes.md) |
| Q6 | Exploratory (Vue vs Svelte rendering) | 6/6: 1.0; ground-truth bonus corrected post-run |

## Bottom line

Equally capable on correctness; the real difference is cost — rtk-gh wins
~1.9x tokens, ~2.3x bytes/wall-time; octocode wins mean quality (4.44 vs 4.08)
driven by the Q5 FP pattern hitting rtk-gh twice vs. octocode once. Pick
rtk-gh for cost, octocode for depth. Both share the same root friction: no
native line numbers from code search (Q2/Q4/Q5). See `judge-notes.md` for the
full false-positive writeup and ground-truth self-correction.
