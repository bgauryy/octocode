# Multi-agent review cost receipt

Load after a two-agent RFC review, whether Jev ran or the workers converged. Why: provider usage alone is not the cost of preparing, debating, judging and verifying a decision.

Write one JSON receipt under `<output>/octocode-rfc-generator/{name}/review/` and run `node scripts/validate-review-cost.mjs receipt.json`. Use this shape:

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
  "judge": {"calls": 1, "attempts": 1, "model": "configured/requested model (pin a version for comparison)", "elapsedMs": 100, "inputTokens": 1000, "outputTokens": 90, "outcome": "judgment"},
  "failures": [],
  "unknowns": ["host.inputTokens", "host.outputTokens", "workers.A.inputTokens", "workers.A.outputTokens", "workers.B.inputTokens", "workers.B.outputTokens"],
  "contribution": "prioritized-existing-concern"
}
```

Use `null`, never zero, for unavailable measurements and list each null metric path in `unknowns`. Worker IDs must be `A` and `B`; record failed/partial workers instead of dropping them. Judge calls may be zero after convergence; then model, elapsed and token fields are null and appear in `unknowns`. Count retries in `attempts` and transport/provider failures in `failures`.

`timing.elapsedMs` covers dispatch preparation through completed host verification. Phase time is observable attribution and may overlap for parallel work; it need not sum to elapsed time. Keep output bytes separate from tokens if recorded elsewhere. Do not calculate total tokens or money while any required component is unknown, and never present judge latency or provider usage as whole-workflow cost.

The receipt improves accounting coverage; it cannot make unavailable host telemetry measurable. For causal value or accuracy claims, use `references/jev-evaluation.md` and a matched, independently graded comparison.

Next: return to `references/jev-review.md` delivery and report what remains unknown.
