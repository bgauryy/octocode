# Learning Loop and Skill Evolution

Load when an outcome must become owned, verified improvement work.

A loop closes only when its output has an owner, an applied action, fresh verification, and a terminal state.

| Trigger | Route | Close |
|---|---|---|
| Reusable verified outcome | `reflect record --lesson` | Recheck later; supersede/archive when stale. |
| Repo behavior gap | `--fix-repo` refinement | Apply, verify, close with receipt. |
| Harness gap | `--fix-harness` memory | Human applies; held-out review/tests pass. |
| Bad instructions | `--fix-instructions` | Update owner, verify live view, close refinement. |
| Repeated failure | stable `--failure-signature` | One cluster, one fix, rerun same signature. |
| Goal/KPI improvement | baseline, smallest change, remeasure | Keep measured improvement without regression. |

Use stable failure keys such as `test:<name>` or `<class>:<site>`, not full messages. `--outcome` is `worked|partial|failed`. Record one concern per developer-review call.

## Housekeeping

Use `query workboard` as the sensor. Preview exact cleanup commands and review IDs before mutation. Prefer supersession, archive, restore, and decay; never infer success from age. Expired unproved runs become failed. Preserve live work, unread signals, and open refinements until their owner acts.

## Skill evolution

Treat the skill folder as trainable external state of a frozen agent:

```text
ATTEND -> GOAL+KPI -> RESEARCH -> PLAN -> USER GATE -> EDIT -> REVIEW -> HELD-OUT CHECK -> REFLECT
```

- Read every behavior-affecting file before editing.
- Prefer one concept per patch and test outside the motivating failure.
- Never accept a plausible diagnosis without held-out validation.
- Keep procedural rules in the skill; instance history belongs in reflection/memory.
- Reject regressions, record why, and try a smaller intervention.
- Release through the owning package or inspected installer; never invent copy paths.

Close refinements with an observed receipt, then re-run `attend` or the targeted query. `export-harness` is preview-only.

Next: use `references/homeostatic-loop.md` for the control model or return to `SKILL.md`.
