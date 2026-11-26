# Octocode 20.2.0 (local MCP) vs `gh`: GitHub 30, 26 November 2026

Both arms answer the 30 GitHub questions equally well. Octocode gets there with 44% fewer tool calls and half
the failed calls; `gh` costs less per answer (Octocode 1.19×, 95% interval 1.09–1.31). Nothing measurable
changed against the round run earlier the same day.

| | Octocode MCP (local) | `gh` CLI | Octocode / `gh` per question |
|---|---:|---:|---:|
| Answers | 90 | 90 (2 invalid) | |
| Quality, 0–10 (valid pairs, 88) | 9.68 | 9.70 | 0 wins, 88 ties, 0 losses |
| Facts right (valid pairs) | 782 of 826 | 787 of 826 | |
| Tool calls | 580 | 1,038 | 0.56× (0.49–0.64), lower on 30 of 30 |
| Failed calls | 15 (2.6%) | 28 (2.7%) | |
| Model calls | 464 | 495 | 0.93× (0.85–1.00) |
| Reply characters | 3,610,280 | 3,109,598 | 1.17× (1.05–1.32) |
| New input tokens | 1,966,402 | 1,532,660 | 1.26× (1.09–1.44) |
| Cached re-read tokens | 15,697,883 | 6,265,877 | 3.02× (2.63–3.44) |
| Output tokens | 377,675 | 386,263 | 0.95× (0.89–1.03) |
| Cost | $13.24 | $11.11 | 1.19× (1.09–1.31), lower on 9 of 30 |
| Time per answer, median | 34.3 s | 32.1 s | |
| Time, total | 4,891 s | 3,988 s | |

Ratios are per-question geometric means over three passes with a 95% bootstrap interval.

## Verdict

| Claim | Verdict | Evidence |
|---|---|---|
| Octocode answers as correctly as `gh` | **Accept** | 88 valid pairs, all tied within one point; 9.68 vs 9.70 |
| Octocode needs fewer tool calls | **Accept** | 0.56×, lower on every question |
| Octocode costs no more than `gh` | **Reject** | 1.19×, lower bound 1.09; per pass 1.27, 1.09, 1.18 |
| The local build changed cost or quality since the last round | **Inconclusive** | every number is inside the last round's interval (below) |

Where the extra cost is: Octocode re-reads three times as many cached tokens (its 14 tool schemas are in
every request: about 24,000 cached tokens on the first call against 5,000 for `gh`) and reads 26% more new
input. One more model call costs $0.0125 for Octocode and $0.0095 for `gh`, before the new input it reads.

## Against the last benchmark

The last round ran earlier on 26 November: the same questions, models and three passes, Octocode candidate
build SHA-256 `eb8f75f2…e612` (campaign `campaigns/v20-base-cand-gh-2026-11-26/`). This round uses the build
staged at 02:02, SHA-256 `cce5ad43…d928`; both report version `20.2.0+91434a43`.

| | Last round: Octocode | This round: Octocode | Last round: `gh` | This round: `gh` |
|---|---:|---:|---:|---:|
| Quality | 9.70 | 9.68 | 9.74 | 9.70 (88 valid) |
| Facts right | 797 of 846 | 801 of 846 | 806 of 846 | 787 of 826 valid |
| Tool calls (failed) | 585 (12) | 580 (15) | 1,005 (46) | 1,038 (28) |
| Model calls | 458 | 464 | 495 | 495 |
| Reply characters | 3,603,185 | 3,610,280 | 3,244,028 | 3,109,598 |
| Cost | $12.79 | $13.24 | $11.18 | $11.11 |
| Time per answer, median | 33.1 s | 34.3 s | 31.0 s | 32.1 s |
| Seconds waited on GitHub limits | 882 | 640 | not measured | not measured |

| Octocode / `gh` | Last round | This round |
|---|---:|---:|
| Cost | 1.14× (1.07–1.22) | 1.19× (1.09–1.31) |
| Tool calls | 0.58× (0.51–0.67) | 0.56× (0.49–0.64) |
| Model calls | 0.92× (0.86–0.98) | 0.93× (0.85–1.00) |
| Reply characters | 1.13× (1.03–1.24) | 1.17× (1.05–1.32) |
| Cached re-reads | 2.89× (2.55–3.24) | 3.02× (2.63–3.44) |

The intervals overlap on every row. About $0.27 of this round's higher Octocode cost is the three cold
sessions listed under [What limits this round](#what-limits-this-round).

## Per pass

| Pass | Octocode quality | `gh` quality | Octocode cost | `gh` cost | Cost ratio | Octocode calls (failed) | `gh` calls (failed) | Octocode waits |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 9.63 | 9.73 | $4.77 | $3.77 | 1.27 (1.11–1.45) | 191 (6) | 355 (6) | 99 s |
| 2 | 9.67 | 9.70 | $4.14 | $3.71 | 1.09 (1.00–1.20) | 193 (5) | 333 (11) | 387 s |
| 3 | 9.73 | 9.03 raw | $4.33 | $3.63 | 1.18 (1.05–1.32) | 196 (4) | 350 (11) | 154 s |

The pass 3 `gh` quality includes the two invalid answers, scored 0. The cost ratio changes side between
passes on Q1, Q6, Q7, Q9, Q12, Q14, Q18, Q20, Q22, Q24 and Q27.

## Failed calls

| Arm | Cause | Calls |
|---|---|---:|
| Octocode | GitHub rate limit (`rateLimited`, code search) | 6 |
| Octocode | `notFound` | 3 |
| Octocode | `gitFailed` 2, `gitUnavailable` 1, `timeout` 1 | 4 |
| Octocode | `invalidInput` 1, refused path outside the allowed roots 1 | 2 |
| `gh` | refused by the host's permission rules | 22 |
| `gh` | HTTP 403 2, HTTP 404 2, non-zero exit 2 | 6 |

## Per question

Scores are pass 1 / pass 2 / pass 3; costs and calls are summed over the three passes; seconds are the mean.

| Question | Title | Octocode score | `gh` score | Octocode cost | `gh` cost | Ratio | Octocode calls | `gh` calls | Octocode s | `gh` s |
|---|---|---|---|---:|---:|---:|---:|---:|---:|---:|
| Q1 | Next.js route-regex result | 10 / 10 / 10 | 10 / 10 / 9 | $0.18 | $0.10 | 1.86 | 6 | 7 | 48 | 12 |
| Q2 | Repository discovery and bounded absence | 9 / 9 / 10 | 10 / 10 / 10 | $0.71 | $0.42 | 1.67 | 42 | 54 | 68 | 39 |
| Q3 | Flask route history | 9 / 9 / 10 | 8 / 8 / 9 | $0.33 | $0.22 | 1.52 | 15 | 27 | 36 | 27 |
| Q4 | Axios redirect implementation across repositories | 10 / 10 / 10 | 10 / 10 / 10 | $0.39 | $0.33 | 1.17 | 16 | 24 | 32 | 32 |
| Q5 | Vue hydration diff review | 10 / 9 / 9 | 10 / 9 / 9 | $1.33 | $0.88 | 1.51 | 39 | 63 | 105 | 106 |
| Q6 | Express router cross-repository trace | 10 / 10 / 10 | 10 / 10 / 10 | $0.29 | $0.29 | 1.01 | 32 | 37 | 34 | 39 |
| Q7 | Zustand's Next.js integration contract | 10 / 10 / 10 | 10 / 10 / 10 | $0.21 | $0.20 | 1.04 | 13 | 28 | 24 | 27 |
| Q8 | VS Code keybinding dispatch | 9 / 10 / 9 | 10 / 9 / 9 | $0.24 | $0.11 | 2.27 | 11 | 12 | 22 | 16 |
| Q9 | Fastify lifecycle contract | 10 / 10 / 10 | 9 / 10 / 9 | $0.20 | $0.22 | 0.90 | 10 | 29 | 45 | 49 |
| Q10 | Axios repository and Node entry chain | 10 / 10 / 9 | 10 / 10 / 10 | $0.27 | $0.18 | 1.47 | 18 | 22 | 27 | 48 |
| Q11 | Esbuild JavaScript-to-Go service boundary | 10 / 10 / 10 | 10 / 10 / 10 | $0.57 | $0.42 | 1.37 | 20 | 38 | 38 | 32 |
| Q12 | Stream and EventEmitter wiring | 10 / 10 / 10 | 10 / 10 / 10 | $0.18 | $0.20 | 0.92 | 7 | 20 | 21 | 26 |
| Q13 | Redis security issue and fix PR | 9 / 9 / 10 | 10 / 10 / 9 | $0.28 | $0.31 | 0.89 | 12 | 33 | 24 | 31 |
| Q14 | Vitest's dependency on Vite | 9 / 9 / 9 | 9 / 9 / 9 | $0.06 | $0.08 | 0.72 | 3 | 9 | 9 | 12 |
| Q15 | Hono JSX array component PR | 9 / 9 / 9 | 9 / 9 / 9 | $0.29 | $0.31 | 0.95 | 19 | 22 | 25 | 24 |
| Q16 | ESLint parser dependency chain | 10 / 10 / 10 | 10 / 10 / 10 | $0.19 | $0.12 | 1.57 | 16 | 17 | 22 | 21 |
| Q17 | Next.js fetch request memoization | 10 / 10 / 10 | 10 / 10 / 10 | $0.42 | $0.33 | 1.29 | 19 | 38 | 48 | 42 |
| Q18 | Vite dependency-section membership | 10 / 10 / 10 | 10 / 10 / 10 | $0.23 | $0.22 | 1.08 | 13 | 21 | 25 | 31 |
| Q19 | Node child-process async and sync paths | 9 / 9 / 9 | 9 / 9 / 9 | $0.51 | $0.45 | 1.13 | 17 | 38 | 35 | 37 |
| Q20 | Actions toolkit exec output path | 10 / 10 / 10 | 10 / 10 / 10 | $0.26 | $0.28 | 0.95 | 8 | 15 | 24 | 26 |
| Q21 | LangChain createAgent flow and graph | 10 / 10 / 10 | 10 / 10 / 10 | $0.93 | $0.59 | 1.57 | 37 | 38 | 91 | 105 |
| Q22 | Axios release-range compare | 8 / 8 / 9 | 9 / 9 / invalid | $0.30 | $0.30 | 1.01 | 14 | 23 | 29 | 78 |
| Q23 | Linux write() syscall to VFS dispatch flow | 10 / 10 / 10 | 10 / 10 / 10 | $0.40 | $0.43 | 0.94 | 13 | 37 | 48 | 47 |
| Q24 | Axios buildFullPath blast radius | 10 / 10 / 10 | 10 / 10 / 10 | $0.45 | $0.43 | 1.04 | 20 | 59 | 45 | 53 |
| Q25 | Axios PR selected-patch and review thread | 9 / 10 / 10 | 9 / 10 / 10 | $0.43 | $0.32 | 1.35 | 18 | 27 | 39 | 33 |
| Q26 | Zustand documentation surface | 10 / 10 / 10 | 10 / 10 / 10 | $0.35 | $0.21 | 1.67 | 20 | 30 | 28 | 23 |
| Q27 | React Query public API surface | 10 / 10 / 10 | 10 / 9 / 10 | $0.40 | $0.45 | 0.89 | 13 | 57 | 46 | 54 |
| Q28 | Hermes agent memory management | 10 / 9 / 9 | 10 / 10 / 10 | $1.40 | $1.60 | 0.87 | 45 | 123 | 140 | 117 |
| Q29 | MCP HTTP transport authorization flow | 10 / 10 / 10 | 10 / 10 / 10 | $0.54 | $0.44 | 1.21 | 27 | 29 | 44 | 33 |
| Q30 | Chromium inter-process communication | 9 / 10 / 10 | 10 / 10 / invalid | $0.92 | $0.70 | 1.31 | 37 | 61 | 409 | 108 |

## What limits this round

| Limit | What happened | Effect |
|---|---|---|
| Two `gh` answers are invalid | In pass 3, Q22 and Q30, a `gh` command that had timed out finished after the session's answer and started a second turn. [run.mjs](../../../harness/run.mjs) keeps the last result of a session, so the stored answer is a two-sentence follow-up (367 and 211 characters), not the report (3,339 and 5,864 characters). The judge scored both 0, twice. | Left out of quality, facts and the win count; counted in cost, calls and time. With them `gh` reads 9.49 and Octocode "wins" 2: a harness defect, not a `gh` result. Q21 pass 3 had the same second turn and kept its full answer. |
| Token usage of those three sessions | The stored usage is the last turn's; the cost is the session's whole cost. | Token columns understate `gh` slightly; the fitted token prices in `tables.md` are unusable for this round (worst session error 92.61%). Costs are the provider's and are unaffected. |
| Three cold Octocode sessions | Q1, Q2 and Q3 in pass 1 wrote the 24,000-token prompt to the cache; every other session read it. | About $0.27 of Octocode's pass 1 cost. Pass 1's ratio (1.27) is overstated; passes 2 and 3 give 1.09 and 1.18. The cache-state gate of [METHOD.md](../../../METHOD.md#accept-a-cost-claim) does not hold for pass 1. |
| GitHub limits under six parallel sessions | Octocode sessions waited 640 s in total on code-search pacing and secondary limits, most of it on Q1, Q2 and Q30; Q30 took 120, 426 and 680 s. `gh` hit network timeouts in pass 3. | Times compare the arms under shared-token contention, not at their best. |
| Development build | The Octocode binary is the `dev` profile build. | Its times are not release timings. |
| Overfitting check | The briefs and live tool instructions were not searched for the questions' terms before this round. | The questions are the ones Octocode is developed against. |

## Round identity

| | |
|---|---|
| Date | 26 November 2026 |
| Octocode | `octocode 20.2.0+91434a43`, SHA-256 `cce5ad4385310619b23ac42ae6bbcdfb066ffb88c1e28355048ffd6a60c5d928`, served by `packages/octocode-mcp/dist/index.js` over stdio |
| Tree | commit `e3ed53555`, with uncommitted changes |
| `gh` | 2.96.0 behind the read-only wrapper ([gh-readonly](../../../harness/gh-readonly)) |
| Questions | [GitHub](../../../questions/github/README.md) Q1–Q30, graded per fact against the answer keys |
| Sessions | 180: 30 questions × 2 arms × 3 interleaved passes, 6 at a time, none failed |
| Runner | `claude-sonnet-5-5`, a fresh session per question and arm |
| Judge | `claude-opus-5-5`, blind, 90 first gradings and 2 swapped re-gradings |
| Manifest | [v20-local-gh-2026-11-26.json](../../../harness/rounds/v20-local-gh-2026-11-26.json) |
| Raw records | `campaigns/v20-local-gh-2026-11-26/` (local): answers, transcripts, verdicts, `analysis/tables.md`, `analysis/numbers.json` |

## Run it again

```bash
DEV='node skills-dev/octocode-dev/scripts/dev.mjs'
ROUND=packages/octocode-benchmark/harness/rounds/v20-local-gh-2026-11-26.json
$DEV bench run $ROUND && $DEV bench judge $ROUND first && $DEV bench judge $ROUND swap && $DEV bench analyze $ROUND
```

A stored answer or verdict is skipped, never overwritten: copy the manifest under a new `round` name for a new round.
