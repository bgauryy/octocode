# Improve Loop

Load when changing **this skill** (or another skill folder) and you need an accept/revert gate. Prefer `octocode-graph-eval` for a full goal→KPI cascade; use this when you just need the loop to be honest.

Investigation loops (Act→Observe→Learn on a code question) are not this — use `loop-mode.md`.

```text
SET GOAL + KPI → SMALLEST CHANGE → MEASURE ACTUAL RESULTS → ACCEPT | REVERT
```

## The gate

1. **Goal + KPI.** Name what should improve and the check that proves it. For this skill the KPI is usually an eval case id, a trigger-corpus outcome, or a link/contract check — something that runs.
2. **Baseline.** Run the check *before* the edit and record the number. No baseline, no accept.
3. **Smallest change.** One reference, one routing line, one rubric row. Bundled edits make the measurement meaningless.
4. **Measure.** Re-run the same check:
   ```bash
   node scripts/check-description.mjs                 # description + trigger corpus
   node scripts/eval-research.mjs --self-test         # all 16 cases, strong vs weak
   node scripts/eval-research.mjs --case <id> -i ans.md
   ```
5. **Accept or revert.** Improved → keep. Flat or worse → revert. Do not keep a change because the reasoning sounded good.

## Reject

- Undefined KPI, or a KPI that cannot fail.
- Narrative-only accept ("this reads better") with no check run.
- Checks written but not executed.
- Editing the eval case to match the answer instead of fixing the guidance.

## Notes

`--self-test` is the regression gate: every strong sample must pass, every weak sample must fail, and the contract checks must hold. If a guidance edit breaks a contract check, the guidance changed meaning — decide that deliberately, do not loosen the check.

Skill folder edits also get package tests plus human review before publishing.
