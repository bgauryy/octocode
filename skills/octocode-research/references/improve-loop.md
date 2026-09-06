# Improve Loop

Load when changing **this skill** (or another skill folder) and you need an accept/revert gate. Prefer `octocode-eval-benchmark` for a full goal→KPI cascade; use this when you need the loop to be honest.

Investigation loops (Act→Observe→Learn on a code question) are not this — use `references/loop-mode.md`.

```text
SET GOAL + KPI → SMALLEST CHANGE → MEASURE ACTUAL RESULTS → ACCEPT | REVERT
```

## The gate

1. **Goal + KPI.** Name what must improve and the check that proves it. For this skill the KPI is usually a description-contract result, a review finding count, or a link/route check — something that runs and can fail.
2. **Baseline.** Run the check *before* the edit and record the number. No baseline, no accept.
3. **Coherent change.** Fix one measured failure family across its owning reference and affected routes. Keep the harness frozen while removing contradictions; record intentional scope changes.
4. **Measure.** Re-run the same check:
   ```bash
   node scripts/check-description.mjs                 # activation contract
   node scripts/check-guidance.mjs --self-test         # guidance regressions
   ```
5. **Accept or revert.** Keep a measured improvement only when guardrails hold. A prose edit can preserve activation checks while improving a separate correctness metric; a flat trigger score alone is not a regression verdict.

## Reject

- Undefined KPI, or a KPI that cannot fail.
- Narrative-only accept ("this reads better") with no check run.
- Checks written but not executed.
- Editing the check to match the answer instead of fixing the guidance.

## Notes

Description and guidance checks must pass. They are offline contract regressions, not proof of agent research effectiveness. Validate query templates from `references/tool-examples.md` against the installed public schemas; exercise representative local/external paths and relevant continuations. Record unsupported capabilities and provider failures.

Run the `octocode-skills` folder review with zero errors. Use package tests/build only when executable package code changes. Before claiming broad effectiveness, evaluate independent tasks with actual outcomes; do not turn keyword checks into a universal quality score.

Next: when the loop is about a code question instead of a skill edit load `references/loop-mode.md`. When the accept decision needs code evidence load `references/code-research.md`. Otherwise the gate ends here — accept or revert, then stop.
