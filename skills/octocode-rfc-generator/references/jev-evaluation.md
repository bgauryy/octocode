# Does the debate improve RFC decisions?

Load before improving this skill or claiming it beats the ordinary RFC flow. Why: more agents and a judge can add cost without changing the outcome.

Freeze the ordinary RFC flow, candidate `clasify` flow, raw case inputs, expected outcomes, rubric, tool schemas, requested model, provider-resolved model, budgets and stopping rule before the scored run. Keep answer keys from executing agents. Give each arm the same cases, evidence access and total resource ceiling, and use fresh contexts. Score the final question dispositions and RFC changes, not the presence of debate vocabulary.

Primary metric: correctly resolved, blocked or deferred consequential questions divided by the predeclared question set. Guardrails: zero unsupported blocker closures, no omitted material counterevidence, no guessed owner decisions, and complete acceptance/dependency/rollback traceability. For workflow amendments, include useful provisional comparison, the next discriminating check, authorized save/edit progress and redundant permission requests in the frozen outcome checks; a safe refusal to do useful authorized work is not a pass. Record useful new questions separately so verbosity cannot inflate the primary denominator.

Include missing evidence, contradictory evidence, a persuasive wrong advocate, stale source revisions, unavailable Jev, a failed worker, an owner-only preference and a trivial direct-check control. At least one executable or exact-source anchor must decide a case. Use held-out cases after development; do not revise graders to reward the candidate's output.

Measure the whole workflow: host and worker tokens when available, provider tokens, requested/resolved models, schema/preparation work, calls/retries, evidence reads, page count, wall time and incomplete attempts. Preserve each page-local answer; do not grade a hidden aggregation. Separate one-time design/evaluation overhead from per-RFC execution cost. Time each arm from dispatch to completed output with the same boundary; record setup, reads, worker rounds, provider requests/retries, grader work and failures separately. Report input/output bytes as bytes, never as token estimates. Unavailable token counts or monetary totals are unknown, not zero; do not call observed wall time a complete economic cost. Jev can judge the debate but must not be the sole evaluator of its own usefulness: use deterministic answer keys, independently inspected evidence and a separate blind grader where judgment is necessary.

To answer “what did Jev help with?”, freeze the host action before seeing its answer, then record the changed action, triggering judgment field, independently checked outcome and discovery origin. Confirmation of an existing plan is not a newly discovered improvement. For a causal estimate, compare the same frozen worker debate with and without Jev in separate fresh host contexts and independently score the final RFC; the ordinary-RFC versus multi-agent comparison does not isolate Jev's effect.

Until such a matched comparison improves the frozen primary metric, describe Jev as risk prioritization or additional verification only. Do not advertise an accuracy improvement from confidence, agreement, a changed check, or a successful provider call.

Predeclare the keep/discard rule. A suitable default is strictly better held-out disposition accuracy with all safety/traceability guards passing inside the shared budget. If both arms tie, do not claim improvement; keep the ordinary path as default and explain the extra cost of optional Jev review. Report a regression honestly and repair it before recommending general use. One small run establishes feasibility or finds failures, not broad superiority, latency savings or statistical significance.

Store fixtures, hashes, rubric, raw outcomes, receipts and a report under `<output>/octocode-eval-benchmark/`. Keep evaluation development files outside the shipped skill. Repeat only for a changed candidate or unresolved variance, not until a favorable judge vote appears.

Next: none — the evaluation ends with the measured verdict; return to `SKILL.md` for delivery.
