# Review cost receipt and evaluation

Load after a two-agent RFC review, whether Jev ran or the workers converged. Load also before improving this skill or claiming that the debate beats the ordinary RFC flow. Provider usage alone is not the cost of preparing, debating, judging, and verifying a decision.

## Cost receipt

Write one JSON receipt under `<output>/octocode-rfc-generator/{name}/review/`. Then run `node scripts/validate-review-cost.mjs receipt.json`. Use this shape:

```json
{
  "version": 1,
  "reviewId": "stable review ID",
  "scope": "single-rfc-review",
  "timing": {"startedAtMs": 0, "completedAtMs": 1200, "elapsedMs": 1200},
  "phases": {"preparationMs": 200, "debateMs": 500, "judgeMs": 100, "verificationMs": 400},
  "host": {"toolCalls": 4, "evidenceReads": 3, "inputTokens": null, "outputTokens": null},
  "workers": [
    {"id": "A", "status": "complete", "rounds": 2, "toolCalls": 2, "evidenceReads": 2, "elapsedMs": 300, "inputTokens": null, "outputTokens": null},
    {"id": "B", "status": "complete", "rounds": 2, "toolCalls": 2, "evidenceReads": 2, "elapsedMs": 320, "inputTokens": null, "outputTokens": null}
  ],
  "judge": {"calls": 1, "attempts": 1, "requestedModel": "configured model", "resolvedModels": ["provider-resolved model/version"], "elapsedMs": 100, "inputTokens": 1000, "outputTokens": 90, "outcome": "judgment"},
  "failures": [],
  "unknowns": ["host.inputTokens", "host.outputTokens", "workers.A.inputTokens", "workers.A.outputTokens", "workers.B.inputTokens", "workers.B.outputTokens"],
  "contribution": "prioritized-existing-concern"
}
```

- Use `null`, never zero, for an unavailable measurement. List each null metric path in `unknowns`.
- Worker IDs must be `A` and `B`. Record failed or partial workers; do not drop them.
- Keep `requestedModel` separate from the unique provider `resolvedModels`. Never infer `resolvedModels` from configuration.
- After convergence, judge calls may be zero. Then `requestedModel` is null, `resolvedModels` is empty, and elapsed and token fields are null and listed in `unknowns`.
- Count retries in `attempts` and transport or provider failures in `failures`. `timing.elapsedMs` covers dispatch preparation through completed host verification. Phase times may overlap for parallel work and need not sum to elapsed time.
- Report bytes as bytes, never as token estimates. Do not calculate total tokens or money while a required component is unknown. Never present judge latency or provider usage as whole-workflow cost.

## Does the debate improve RFC decisions?

The receipt improves accounting coverage. Causal value or accuracy claims need a matched, independently graded comparison:

1. Before the scored run, freeze both flows (ordinary RFC and candidate `clasify`), raw case inputs, expected outcomes, rubric, tool schemas, requested and resolved models, budgets, and stopping rule. Keep answer keys from executing agents.
2. Give each arm the same cases, evidence access, total resource ceiling, and fresh contexts. Score final question dispositions and RFC changes, not debate vocabulary.
3. Primary metric: correctly resolved, blocked, or deferred consequential questions divided by the predeclared question set. Record useful new questions separately so verbosity cannot inflate the denominator.
4. Guardrails: zero unsupported blocker closures, no omitted material counterevidence, no guessed owner decisions, and complete acceptance, dependency, and rollback traceability. For workflow amendments, also check useful provisional comparison, the next discriminating check, authorized save or edit progress, and redundant permission requests. A safe refusal to do useful authorized work is not a pass.
5. Include cases with missing evidence, contradictory evidence, a persuasive wrong advocate, stale source revisions, unavailable Jev, a failed worker, an owner-only preference, and a trivial direct-check control. At least one executable or exact-source anchor must decide a case. Use held-out cases after development; do not revise graders to reward the candidate.
6. Measure the whole workflow per arm with the same boundary: host and worker tokens when available, provider tokens, models, preparation, calls and retries, evidence reads, pages, wall time, grader work, and incomplete attempts. Separate one-time design overhead from per-RFC cost. Unknown tokens or money stay unknown, not zero.
7. Jev must not be the sole evaluator of its own usefulness. Use deterministic answer keys, independently inspected evidence, and a separate blind grader where judgment is necessary.
8. To isolate Jev's effect, compare the same frozen worker debate with and without Jev in separate fresh host contexts. Freeze the host action before seeing Jev's answer; record the changed action, triggering field, checked outcome, and discovery origin. Confirmation of an existing plan is not a new improvement.

Keep/discard rule: keep the candidate only with strictly better held-out disposition accuracy AND fewer total host tokens, with all guards passing. Otherwise, or when host tokens are unavailable, keep the ordinary path as default and keep this an explicitly requested experiment. One small run shows feasibility or failures, not broad superiority or significance. Store fixtures, hashes, rubric, raw outcomes, receipts, and the report under `<output>/octocode-eval-benchmark/`, outside the shipped skill.

Next: return to `references/jev-review.md` delivery and report what remains unknown.
