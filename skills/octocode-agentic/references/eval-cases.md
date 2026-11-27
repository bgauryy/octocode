Load when building eval cases from failures, using a public benchmark, or splitting tuning from sealed-test cases.

# Eval cases and splits

## Cases from failures

1. Collect representative traces (production, dogfood, or synthetic starter): 20–50 tasks drawn from real failures are enough to start (Anthropic); a domain expert notes each one's first clear failure. Fix a task set from real workloads, including hard and adversarial cases, before changing anything.
2. Cluster into a failure taxonomy with frequencies until new traces stop changing it; inspect rare high-impact failures separately.
3. One grader or case family per top failure mode, grouped by the observed mechanism; prioritize frequency and impact while keeping rare critical failures as guardrails.

- Keep representative samples apart from enriched stress tests.
- Cover both sides of a behavior: cases where it should happen and cases where it should not. Give every task a reference solution that proves it is solvable.
- Cascading failures: fix or tag the first upstream break. Product or model shift: redo the analysis.

## Public benchmarks

- Reported public gain: audit transcripts, else weak evidence. Check it measures your skill.
- Check exposure through training, prompts or retrieval. Near-ceiling or uniformly failing tasks need solvability and grader checks before interpretation.
- SWE-style coding: passing tests are not merge-ready (in one audit about half of test-passing changes were not mergeable); a saturated board is never sole proof. An OpenAI audit found flawed tests in 59.4% of the SWE-bench Verified problems it reviewed.
- Contaminated, saturated, or gamed: retire to regression smoke; build a fresh private suite.

## Splits and exposure

- **Development**: inspect failures, tune within budget. **Validation**: select candidates under a fixed budget.
- **Sealed test**: confirm the selected candidate once under the preregistered plan, with both arms under identical conditions; no tuning. **Regression**: known successes that must stay correct; not generalization evidence.
- Split before making variants, by source problem, repository, customer, template, or time; near-duplicates and paraphrases share a split.
- Synthetic cases supplement real ones after solvability and reference checks; template variants are not independent.
- Record split version and every sealed-test access; repeated pass/fail summaries leak too.
- Sealed-test feedback guided a change: retire those items to development or regression; get fresh confirmation tasks. Private is not unexposed.
