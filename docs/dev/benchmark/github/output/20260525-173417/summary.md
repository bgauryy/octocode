# Benchmark summary — octocode vs gh

Two agents answered all 46 GitHub research questions in `docs/dev/benchmark/github/QUESTIONS.md`: `octocode` using Octocode MCP tools and `gh` using the GitHub CLI wrapper. The `gh` run won quality decisively on the non-drift set: 101/111 points vs octocode's 61/111. Octocode was dramatically cheaper and faster, winning tied-quality questions on efficiency, but too many answers were placeholders or generic summaries rather than concrete researched facts. The headline: `gh` won on answer quality; `octocode` won on cost/latency.

## Per-question table

| Q | Drift | Octo qual | gh qual | Octo calls | gh calls | Octo chars | gh chars | Octo q_ms | gh q_ms | Winner | Notes |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| 1 |  | 2/3 | 2/3 | 1 | 2 | 1,591 | 18,772 | 49,719 | 18,184 | octocode | Both missed `ReactSharedInternals.H` and mount/update dispatcher swap. |
| 2 |  | 1/3 | 1/3 | 1 | 13 | 5,299 | 713,596 | 5,088 | 86,770 | octocode | Both confused/missed `AppRouterInstance` and TanStack `RouterHistory.push`. |
| 3 |  | 2/3 | 2/3 | 1 | 4 | 912 | 112,949 | 1,290 | 34,899 | octocode | Both omitted `attachPingListener` / exact `ShouldCapture` detail. |
| 4 |  | 2/3 | 2/3 | 1 | 4 | 3,648 | 95,659 | 1,222 | 31,983 | octocode | Both missed some exact sites; octocode misplaced push implementation. |
| 5 |  | 2/3 | 2/3 | 1 | 13 | 5,315 | 101,458 | 5,137 | 96,212 | octocode | Both missed Next `resume()`/PPR and Nuxt import nuance. |
| 6 |  | 2/3 | 3/3 | 1 | 6 | 2,190 | 21,287 | 1,323 | 45,446 | gh | Octocode omitted `allReady`, `signal`, `onHeaders`, `maxHeadersLength`. |
| 7 |  | 1/3 | 3/3 | 1 | 2 | 2,257 | 818,234 | 2,399 | 45,649 | gh | Octocode gave generic imports; gh enumerated concrete modules. |
| 8 |  | 3/3 | 3/3 | 1 | 2 | 2,034 | 9,180 | 1,826 | 19,707 | octocode | Both captured overloads and mutator generics. |
| 9 |  | 2/3 | 2/3 | 1 | 1 | 2,593 | 2,766 | 1,569 | 16,011 | octocode | Both partial; gh more complete but still missed `skills/`. |
| 10 |  | 2/3 | 3/3 | 1 | 1 | 8,394 | 3,467 | 5,253 | 18,745 | gh | Octocode missed `mcp/`, `og/`, `after/`, `use-cache/` details. |
| 11 |  | 2/3 | 3/3 | 1 | 3 | 4,176 | 1,583 | 1,522 | 29,018 | gh | Octocode gave categories rather than package lists. |
| 12 | ✓ | d:1/3 | d:3/3 | 1 | 3 | 3,223 | 517 | 5,635 | 23,238 | — | Octocode gave no current numbers; gh gave stars/push/issues. |
| 13 | ✓ | d:1/3 | d:2/3 | 1 | 1 | 2,679 | 966 | 2,107 | 22,029 | — | Octocode gave no concrete repos; gh list was useful but broad/noisy. |
| 14 | ✓ | d:1/3 | d:3/3 | 1 | 3 | 10,607 | 2,081 | 2,603 | 28,426 | — | Octocode included stale/wrong PRs; gh had current rust-compiler stack. |
| 15 |  | 0/3 | 3/3 | 1 | 3 | 26,266 | 16,485 | 3,618 | 33,225 | gh | Octocode was vague; gh captured `attach`, files, disagreement, stale status. |
| 16 | ✓ | d:0/3 | d:3/3 | 1 | 2 | 18,768 | 1,071 | 4,461 | 15,865 | — | Octocode output placeholders; gh gave concrete hydration PRs. |
| 17 |  | 0/3 | 2/3 | 1 | 22 | 13,886 | 1,934,016 | 2,743 | 230,908 | gh | Octocode placeholder; gh gave #41629 but missed current peerDep evidence. |
| 18 |  | 2/3 | 2/3 | 1 | 4 | 4,308 | 27,287 | 3,117 | 35,369 | octocode | Both missed full `ReactFizzServer`/`resume()`/RSC separation. |
| 19 |  | 2/3 | 3/3 | 1 | 3 | 4,278 | 756 | 5,029 | 21,052 | gh | Octocode partial paths; gh confirmed exact adoption/not-found. |
| 20 |  | 2/3 | 3/3 | 1 | 14 | 3,064 | 1,696,752 | 3,101 | 115,428 | gh | Octocode lacked scheduler/complexity detail; gh covered patch vs fiber/lane. |
| 21 |  | 2/3 | 2/3 | 1 | 5 | 1,550 | 729,305 | 3,304 | 49,980 | octocode | Both missed developer gotchas; core dependency tracking was right. |
| 22 |  | 2/3 | 3/3 | 1 | 5 | 3,945 | 41,205 | 1,713 | 44,062 | gh | Octocode generic; gh identified `source`/`state`/`set` details. |
| 23 |  | 2/3 | 3/3 | 1 | 4 | 953 | 28,708 | 1,584 | 36,502 | gh | Octocode missed codegen ternary; gh covered `transformIf`/branches. |
| 24 |  | 2/3 | 3/3 | 2 | 7 | 2,090 | 263,921 | 4,085 | 65,603 | gh | Octocode vague on `experimental`; gh listed schema keys extensively. |
| 25 |  | 2/3 | 3/3 | 1 | 4 | 3,018 | 2,349 | 2,843 | 47,203 | gh | Octocode omitted `module-runner`/types specifics; gh covered subsystems. |
| 26 | ✓ | d:1/3 | d:3/3 | 2 | 5 | 6,034 | 863 | 9,200 | 20,231 | — | Octocode gave no values; gh ranked all five with metrics. |
| 27 |  | 0/3 | 3/3 | 1 | 10 | 10,228 | 10,158 | 2,847 | 82,174 | gh | Octocode placeholder; gh gave PR #25084 and files. |
| 28 | ✓ | d:0/3 | d:3/3 | 1 | 1 | 9,342 | 780 | 2,516 | 13,161 | — | Octocode placeholder; gh listed five concrete PRs. |
| 29 |  | 2/3 | 3/3 | 1 | 6 | 3,519 | 160,771 | 3,147 | 51,827 | gh | Octocode partial PPR flow; gh covered `unstable_postpone`/resume/state. |
| 30 |  | 2/3 | 3/3 | 1 | 9 | 1,766 | 402,337 | 1,406 | 73,242 | gh | Octocode generic; gh named `SignalState`, `Listener`, `writeSignal`. |
| 31 |  | 2/3 | 3/3 | 1 | 9 | 4,475 | 79,687 | 3,158 | 79,500 | gh | Octocode lacked concrete `$.state` / `RefImpl` details. |
| 32 |  | 2/3 | 3/3 | 1 | 1 | 1,431 | 10,534 | 1,578 | 11,314 | gh | Octocode omitted `_parse`, `$ZodAsyncError`, `finalizeIssue`. |
| 33 |  | 2/3 | 3/3 | 1 | 1 | 1,811 | 3,049 | 2,113 | 7,585 | gh | Octocode omitted signature, `onError`, and `onNotFound`. |
| 34 |  | 2/3 | 3/3 | 1 | 1 | 2,983 | 8,822 | 1,919 | 7,804 | gh | Octocode omitted `_config`, server guard, defaults. |
| 35 |  | 1/3 | 3/3 | 1 | 2 | 3,709 | 87,778 | 1,986 | 17,129 | gh | Octocode was too generic; gh covered lifecycle and retry/repeat loops. |
| 36 |  | 2/3 | 3/3 | 1 | 1 | 3,464 | 7,140 | 1,983 | 8,627 | gh | Octocode overbroad `HydrationMetadata`; gh gave exact fields. |
| 37 |  | 1/3 | 3/3 | 1 | 1 | 3,005 | 7,638 | 1,962 | 9,029 | gh | Octocode lacked exact signature/imports; gh gave them. |
| 38 |  | 1/3 | 3/3 | 1 | 2 | 817 | 639 | 1,494 | 12,427 | gh | Octocode omitted package list; gh listed exact packages. |
| 39 |  | 2/3 | 3/3 | 1 | 2 | 1,735 | 3,111 | 1,547 | 14,786 | gh | Octocode generic; gh named `crates`, `e2e-tests`, `fuzz`, `plugins`, `xtask`. |
| 40 | ✓ | d:1/3 | d:3/3 | 1 | 3 | 4,137 | 826 | 5,507 | 9,528 | — | Octocode gave no values; gh gave concrete stars/push/issues. |
| 41 | ✓ | d:0/3 | d:3/3 | 1 | 1 | 8,527 | 651 | 2,660 | 5,726 | — | Octocode placeholder; gh listed five PRs. |
| 42 |  | 2/3 | 3/3 | 1 | 2 | 4,158 | 60,234 | 2,170 | 13,846 | gh | Octocode missed `executionCtx` and streaming-helper nuance. |
| 43 |  | 1/3 | 3/3 | 1 | 2 | 2,472 | 2,421 | 1,872 | 9,883 | gh | Octocode omitted package names; gh listed exact packages. |
| 44 |  | 2/3 | 3/3 | 1 | 5 | 1,999 | 1,644,106 | 3,224 | 27,023 | gh | Octocode partial islands/RSC distinction; gh detailed streaming/model. |
| 45 |  | 0/3 | 3/3 | 1 | 3 | 23,436 | 51,430 | 3,514 | 23,023 | gh | Octocode hallucinated a React Query/Suspense fix; gh found proxy coercion fix. |
| 46 | ✓ | d:1/3 | d:3/3 | 2 | 4 | 4,477 | 1,061 | 7,144 | 9,367 | — | Octocode gave no values; gh gave concrete ranking/metrics. |

## Quality verdict (non-drift Qs only)

| Agent | Σ quality | Wins | Ties | Avg per Q |
|---|---:|---:|---:|---:|
| octocode | 61/111 | 0 | 9 | 1.65 |
| gh | 101/111 | 28 | 9 | 2.73 |

The `gh` agent handled code archaeology, PR review, exact package layouts, and signature-level source questions much better. Octocode's best category was quick structural/code-location questions where a concise answer was enough (Q1, Q3, Q8, Q18, Q21), but it repeatedly failed drift/current-metadata questions and PR-review questions. The closest non-drift questions were Q1–Q5, Q8–Q9, Q18, and Q21, where quality tied and octocode won the Pareto rule by using fewer chars.

## Drift verdict (reported separately)

| Agent | Σ drift quality |
|---|---:|
| octocode | 6/27 |
| gh | 26/27 |

Octocode answered most drift questions with meta-instructions instead of results, especially Q16, Q28, and Q41. `gh` answered drift questions well overall; Q13 was the only loose case because the repo set is inherently broad and included AI/testing-adjacent projects.

## Efficiency verdict

| Axis | octocode | gh | ratio (octo/gh) |
|---|---:|---:|---:|
| Σ calls | 49 | 202 | 0.24x |
| Σ in_chars (per-Q) | 20,241 | 23,325 | 0.87x |
| Σ out_chars (per-Q) | 220,328 | 9,165,081 | 0.02x |
| MCP init chars | 339,325 | 0 | — |
| TOTAL chars (per-Q + init) | 579,894 | 9,188,406 | 0.06x |
| Σ tool_elapsed_ms | 108,095 | 244,314 | 0.44x |
| Σ q_elapsed_ms | 185,238 | 1,718,746 | 0.11x |
| Σ reasoning_ms | 77,143 | 1,474,432 | 0.05x |

Octocode used about 6.3% of gh's total chars even after including the MCP initialization cost. MCP init was 339,325 chars, about 58.5% of octocode's total benchmark character cost; without init, octocode's per-question work was only 240,569 chars. Octocode was also much faster end-to-end: 185s vs 1,719s total Q wall time. However, much of that economy came from shallow or placeholder answers, so cheap wrong answers should not be treated as wins.

## Failure-mode review

- **Placeholders instead of answers (octocode):** Q16 said “the most recent merged PR ... is the top merged result” without giving a PR number/title/date; Q28 and Q41 used the same placeholder pattern for last-five-merged-PR questions.
- **Confident but wrong PR review (octocode Q45):** it claimed PR #7336 changed “React Query/client integration and tests/examples” and involved “suspense/thenable/use and external-store rendering behavior.” The expected and gh-supported issue was server-side `createInnerProxy` proxy coercion (`valueOf`, `toString`, `toJSON`) in React 19.
- **Vague answer where concrete facts were required (octocode Q15):** it said the issue was a generic “compatibility and architectural tradeoff” rather than identifying `react-devtools-inline` `attach`, the `__REACT_DEVTOOLS_ATTACH__` disagreement, and stale/open status.
- **Current-metadata failures (octocode):** Q12, Q13, Q26, Q40, and Q46 repeatedly said to compare `stargazersCount`, `pushedAt`, and `openIssuesCount` but did not report the values.
- **Missing exact signatures/imports (octocode):** Q7, Q35, Q37, Q38, and Q43 were under-scored because they summarized categories rather than listing exact imports, function signatures, package names, or loop structure.
- **gh cost blowups:** gh achieved better quality but sometimes at huge output cost: Q17 used 1,934,016 chars, Q20 used 1,696,752, and Q44 used 1,644,106. These were quality wins but extremely inefficient.
- **Bad/ambiguous question note:** Q13 is broad: “TypeScript testing repos” admits AI-eval, browser automation, and test-runner interpretations. I scored it loosely as drift and did not exclude it beyond the normal drift treatment.

## Verdict

`gh` won the benchmark on quality by a wide margin: 101/111 vs 61/111 on non-drift questions, with 28 quality wins and 9 ties. Octocode won efficiency decisively, using roughly 6% of gh's total chars and 11% of its end-to-end wall time, but many octocode answers were generic or placeholders. The practical conclusion is that this octocode run was excellent for cheap retrieval but unreliable for deep, exact research unless the agent is forced to extract concrete facts. Do not combine the axes into one composite score: choose `gh` for correctness here, and octocode for cost/latency only when answer depth is sufficient.
