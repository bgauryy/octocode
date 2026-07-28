# Goal → KPI Cascade
Load when linking a user goal to measurable KPIs before looping. Why: orphan KPIs optimize the wrong thing.

## Cascade
```text
USER GOAL (outcome)
  → SUCCESS CRITERIA (observable done)
    → PRIMARY KPI (lagging: did we win?)
      → LEADING KPIs (drivers you can move this week)
        → GUARDRAILS (must not regress)
          → DECISION RULE (ACCEPT / REVERT / CONTINUE)
```

| Layer | Question | Example |
|---|---|---|
| Goal | What user-visible win? | Fewer false skill triggers |
| Success | How would a human verify? | Held-out prompts fire correctly |
| Primary (lagging) | Final scoreboard | false-trigger rate ↓ |
| Leading | Early signal | description trigger-eval pass@1 ↑ |
| Guardrail | What must not break | true-trigger recall ≥ floor; review ERROR=0 |

## Rules
1. Every KPI names the **goal section** it serves — no metric without a parent goal.
2. Prefer one primary lagging KPI; 1–3 leading max.
3. Leading without lagging → local optima; lagging without leading → slow feedback.
4. Decision rule is binary enough to keep/discard: `ACCEPT if primary≥target AND guardrails hold`.
5. If the goal is fuzzy, stop and clarify — do not invent a vanity KPI.

## Anti-patterns
Optimizing tokens while the goal was correctness · dashboard metrics with no decision rule · multiple “primaries”

Next: write the contract fields → `kpi-contract.md`; run loops → `nested-loops.md`.
