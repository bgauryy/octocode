# Improve Loop
**Owner:** this skill (`octocode-eval`). Other Octocode skills keep a stub and route here.
Load when improving a skill, harness docs, or process outcome. Why: improvement is a control loop, not a rewrite from vibes.

Thesis (Awareness / harness): `Agent = Model + Harness`. Improve the harness; measure artifacts. Homeostasis needs **goal → KPI → act → remeasure** against observable sensors — see package `docs/THESIS.md` when Awareness is in play.

## Contract

```text
SET GOAL + KPI → SMALLEST CHANGE → MEASURE ACTUAL RESULTS → COMPARE TO THESIS/TARGETS → ACCEPT | REVERT | NEXT KPI
```

1. **Goal** — one user-visible outcome (e.g. fewer wrong triggers, higher Recall@3, 0 ERROR on skill-review, green held-out smoke).
2. **KPI** — measurable signal with baseline + target (eval case score, test pass, attend compact bytes, verify debt, review ERROR count). Prefer ranges from thesis pressures when applicable (token, verification, memory, harness).
3. **Change** — smallest patch to lobby/ref/script/docs; no unbounded rewrite.
4. **Actual results** — run real checks (`eval-*.mjs`, tests, `skill-review`, `attend`/`verify`).
   Use a held-out task **not** used to invent the edit.
   Prefer deterministic CLI/code with recorded exit codes. **Results > words.**
5. **Thesis check** — confirm no regression on evidence discipline, user authority, safety guards, or “retrieved memory is a lead.” If Awareness thesis pressures apply, remeasure the matching sensor.
6. **Decide** — accept only if KPI moves and thesis guards hold; else revert, record what failed, pick a smaller KPI.

## Stop / reject

- KPI undefined or only subjective (“feels better”).
- Checks not run, or only synthetic fixtures that overfit.
- Claims backed by prose/summaries without deterministic execution or exact evidence.
- Thesis guard broken (silent policy merge, unverified success, hidden omissions).

## Output

`Goal`, `KPI (baseline→result)`, `Checks run`, `Thesis guard`, `Verdict` (accept/revert), `Next KPI` if any.

Next: for skill folder edits load `octocode-skills` improve/review; for Awareness harness evolution load Awareness `skill-evolution`.
