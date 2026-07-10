# Benchmarking
Load when choosing or trusting a benchmark / public suite. Why: leaderboards lie when contaminated or saturated.

## Public vs private
| Kind | Role |
|---|---|
| **Public bench** | Rough capability signal; compare systems; weak for product ship gates |
| **Private suite** | Real failures from *your* traces; primary ship gate |
| **Hybrid** | Public for orientation; private for ACCEPT/REVERT |

Prefer private suites sourced from error analysis. Public gains without transcript audit = **weak**.

## Hygiene checklist
- **Construct validity** — does the bench measure the skill you care about?
- **Contamination** — test items (or paraphrases) leaked into train/prompts/RAG? Assume risk on famous benches.
- **Saturation** — scores near ceiling → no hill; graduate to harder tasks or new suite.
- **Grader bugs** — 0% pass@100 often means broken task/grader, not a dumb agent.
- **Outcome tests** — coding: fail-to-pass (bug fixed) + pass-to-pass (no regressions). Tests alone ≠ merge-ready (quality/taste still needed).

## Coding-bench pattern (SWE-style)
1. Issue + repo snapshot  
2. Agent patch  
3. Run fail-to-pass + pass-to-pass tests  
4. Read transcripts; distrust saturated Verified-style boards as sole proof  

## When to retire a bench
Contaminated, saturated, or gaming-dominated → keep as regression smoke only; build a fresh private capability suite.

Next: write cases from failures → `error-analysis.md`; splits → `held-out-and-guards.md`.
