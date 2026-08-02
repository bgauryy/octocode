# KPI Contract
Load when filling measurable fields after the goal→KPI cascade. Why: no filled contract → no comparable runs.

## Required fields
| Field | Rule |
|---|---|
| **Goal** | One user-visible outcome (parent of every KPI) |
| **Primary KPI** | Single lagging number/pass rate serving that goal |
| **Direction** | higher-better or lower-better |
| **Baseline** | Measured *before* the change |
| **Target** | Explicit threshold or delta |
| **Leading** (optional) | 1–3 drivers with faster feedback |
| **Budget** | Fixed eval cost (time, tokens, trials) so runs compare |
| **Guardrails** | Counter-metrics that must not regress |
| **Held-out** | Cases **not** used to invent the patch |
| **Decision rule** | `ACCEPT if … else REVERT` |

## Good primary KPIs
- Deterministic: test pass rate, `skill-review` ERROR count, case score, attend bytes, verify debt
- Reliability: pass@1 (one-shot) or pass^k (consistency)
- Coding suites: fail-to-pass rate with pass-to-pass guardrail

## Bad KPIs
- “Feels better”, stars alone, saturated public benches without transcript audit
- Editing cases/graders until green · orphan metrics with no parent goal

## Output shape
```text
Goal: …
Primary KPI: <name> (<dir>) baseline=… target=…  [serves goal]
Leading: …
Guardrails: …
Budget: … Held-out: … Decision: ACCEPT if …
```

Next: cascade depth → `goal-kpi-cascade.md`; run → `agent-loop.md`.
