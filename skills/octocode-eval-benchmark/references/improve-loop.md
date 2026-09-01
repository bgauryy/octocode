# Improve Loop
**Owner:** this skill (`octocode-eval-benchmark`). Other Octocode skills keep a stub and route here.
Load when improving a skill, harness, docs, or process outcome. Why: accept changes from measured behavior, not prose preference.

## Contract

```text
SET GOAL + KPI → SMALLEST CHANGE → MEASURE ACTUAL RESULTS → COMPARE TO THESIS/TARGETS → ACCEPT | REVERT | NEXT KPI
```

1. Define one user-visible goal, measurable KPI, baseline, target, guardrails, held-out set, and budget.
2. Make the smallest relevant change without altering the frozen harness.
3. Run real checks with recorded results; prefer deterministic CLI/code evidence.
4. Confirm evidence discipline, user authority, and safety boundaries did not regress.
5. Accept only if the KPI moves and guardrails hold; otherwise revert or narrow the hypothesis.

## Stop / reject

- KPI undefined or only subjective (“feels better”).
- Checks not run, or only synthetic fixtures that overfit.
- Claims backed by prose/summaries without deterministic execution or exact evidence.
- Evidence or authority guard broken (silent policy merge, unverified success, hidden omissions).

## Output

`Goal`, `KPI (baseline→result)`, `Checks run`, `Guardrails`, `Verdict` (accept/revert), `Next KPI` if any.

Next: for skill folder edits load `octocode-skills` improve/review; for Awareness harness evolution load Awareness `skill-evolution`.
