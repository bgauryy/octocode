# Improve Loop

Load when changing **this skill** (or another skill folder) and you need an accept/revert gate. Prefer `octocode-eval-benchmark` for a full goal→KPI cascade; use this when you just need the loop to be honest.

Investigation loops (Act→Observe→Learn on a code question) are not this — use `references/loop-mode.md`.

```text
SET GOAL + KPI → SMALLEST CHANGE → MEASURE ACTUAL RESULTS → ACCEPT | REVERT
```

## The gate

1. **Goal + KPI.** Name what should improve and the check that proves it. For this skill the KPI is usually a description-contract result, a review finding count, or a link/route check — something that runs and can fail.
2. **Baseline.** Run the check *before* the edit and record the number. No baseline, no accept.
3. **Smallest change.** One reference, one routing line, one rubric row. Bundled edits make the measurement meaningless.
4. **Measure.** Re-run the same check:
   ```bash
   node scripts/check-description.mjs                 # description + trigger corpus
   ```
5. **Accept or revert.** Improved → keep. Flat or worse → revert. Do not keep a change because the reasoning sounded good.

## Reject

- Undefined KPI, or a KPI that cannot fail.
- Narrative-only accept ("this reads better") with no check run.
- Checks written but not executed.
- Editing the check to match the answer instead of fixing the guidance.

## Notes

`node scripts/check-description.mjs` is the regression gate: every contract check must hold. If a guidance edit breaks one, the guidance changed meaning — decide that deliberately, do not loosen the check. If you add a trigger corpus, every strong sample must pass and every weak sample must fail.

Skill folder edits also get package tests plus human review before publishing.

Next: when the loop is about a code question instead of a skill edit load `references/loop-mode.md`; when the accept decision needs code evidence load `references/code-research.md`; otherwise the gate ends here — accept or revert, then stop.
