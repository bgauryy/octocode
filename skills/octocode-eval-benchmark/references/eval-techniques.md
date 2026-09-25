# Eval techniques
Load when selecting graders and metrics. Why: match the measurement to the claim.

A **task** is a problem; a **trial** is one attempt; a **trace** records actions; an **outcome** is the resulting artifact/environment state. A grader measures that evidence. The agent harness runs the subject; the eval harness runs and grades trials.

| Grader | Use | Limitation |
|---|---|---|
| Executable outcome check | Tests, state, schema, exact facts with sensible tolerance | A passing test can miss requirements or reward a shortcut |
| Binary criteria | Atomic, interpretable failures | A regex matching the words is not a semantic judgment |
| LLM rubric / pairwise | Open-ended quality or preference | Requires calibration, blinding, bias checks, and abstention |
| Human review | Domain labels, ambiguous cases, critical disputes | Reviewers can disagree; record and adjudicate |
| Grader DAG | Cheap checks can short-circuit later expense | Preserve failed/unknown/skipped states and frozen aggregation |

Use deterministic checks for what they actually establish, and `llm-judge.md` for model judgments. If a judge generates grading steps (G-Eval-style), develop and freeze those steps before the comparison; do not invent a new rubric for every candidate.

For coding, require both fail-to-pass fixes and pass-to-pass regression checks. For tools, grade semantic arguments, scope, transport validity, and final state; a tool name alone proves little. Use `trajectory-grading.md` when a process constraint is part of the task.

## Metrics that answer different questions
- **pass@1:** one-attempt success under the actual deployment budget.
- **pass@k:** chance at least one of k attempts succeeds; include the k-attempt cost and selection/oracle assumption.
- **pass^k:** chance all k attempts succeed; reliability across repeated trials.

Estimate these from a declared repeated-trial design, not by raising a heterogeneous aggregate success rate to a power. Track quality, cost, latency, and critical failure slices separately. Capability tasks reveal headroom; stable known successes form regression checks. Include both positive and negative activation cases.

Next: uncertainty and stopping → `held-out-and-guards.md`; cases and runner → `eval-harness.md`.
