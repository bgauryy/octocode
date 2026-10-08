# Error analysis and benchmarks

Load before you write new eval cases, when the suite feels generic, or when you choose or trust a public benchmark. Evals come from real failure modes, not vanity metrics.

1. **Dataset**: gather representative traces (production, dogfood, or a synthetic starter).
2. **Open coding**: a domain expert notes the *first* clear failure per trace.
3. **Axial coding**: cluster notes into a failure taxonomy; count frequency.
4. **Saturation**: stop when more representative traces no longer change the taxonomy. Inspect rare high-impact failures separately.
5. **Write evals**: one grader or case family per top failure mode, with a `failureSignature` (`mechanism:…|cause:…`) for mining and host verification records.

- Skip generic platform metrics (toxicity, helpfulness) unless your taxonomy has them.
- Rank by frequency × impact; make rare critical failures explicit guardrails. Keep representative sampling separate from enriched stress tests.
- Revisit after product or model shifts. Fix or tag the first upstream break; it causes downstream noise.

## Public benchmarks

- A public gain without a transcript audit is weak evidence.
- Check construct validity: does the benchmark measure the skill you care about?
- Assume contamination on famous benchmarks (items or paraphrases in training, prompts, or RAG).
- Scores near ceiling leave no hill: graduate to harder tasks or a new suite.
- 0% pass@100 often means a broken task or grader, not a weak agent.
- Coding (SWE-style): issue + repository snapshot → agent patch → tests → read transcripts. Passing tests are not merge-ready; a saturated board is never sole proof.
- Retire a contaminated, saturated, or gaming-dominated benchmark to regression smoke; build a fresh private capability suite.
