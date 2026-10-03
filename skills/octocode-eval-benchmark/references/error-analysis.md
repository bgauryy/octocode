# Error analysis and benchmarks
Load before you write new eval cases, when the suite feels generic, or when you choose or trust a public benchmark. Evals come from real failure modes, not vanity metrics.

## Process
1. **Dataset**: gather representative traces (production, dogfood, or a synthetic starter).
2. **Open coding**: a domain expert notes the *first* clear failure per trace.
3. **Axial coding**: cluster notes into a failure taxonomy; count frequency.
4. **Saturation**: stop when more representative traces no longer change the taxonomy. Inspect rare high-impact failures separately.
5. **Write evals**: one grader or case family per top failure mode; attach a `failureSignature` (`mechanism:…|cause:…`) for mining and host verification records.

- Before you assign fixes, use `references/failure-repair.md`: task, grader, infrastructure, leakage, and solver failures can overlap. Record the first divergence and contributing factors.
- Outputs: the taxonomy prioritizes what to measure; top-N modes become capability suite targets; new cases grow the suite loop.
- Do not start from generic platform metrics (toxicity, helpfulness) unless they appear in your taxonomy.
- Prioritize frequency × impact; make rare critical failures explicit guardrails. Keep representative sampling separate from enriched stress tests.
- Revisit after product or model shifts. Upstream errors cause downstream noise: fix or tag the first break.
- Error analysis feeds the suite loop; experiments then hill-climb those cases; the meta loop changes the program when the same signatures recur.

## Public and private benchmarks
| Kind | Role |
|---|---|
| Public | Rough capability signal; compare systems; weak ship gate |
| Private | Real failures from your traces; primary ship gate |
| Hybrid | Public for orientation; private for ACCEPT/REVERT |

- Prefer private suites sourced from error analysis. A public gain without a transcript audit is weak evidence.
- Check construct validity: does the benchmark measure the skill you care about?
- Assume contamination risk on famous benchmarks (items or paraphrases in training, prompts, or RAG).
- Saturation: scores near ceiling leave no hill; graduate to harder tasks or a new suite.
- 0% pass@100 often means a broken task or grader, not a weak agent.
- Coding (SWE-style): issue + repository snapshot → agent patch → fail-to-pass and pass-to-pass tests → read transcripts. Passing tests alone are not merge-ready; do not trust a saturated board as sole proof.
- Retire a contaminated, saturated, or gaming-dominated benchmark to regression smoke; build a fresh private capability suite.

Next: add cases → `references/eval-harness.md`; splits → `references/held-out-and-guards.md`.
