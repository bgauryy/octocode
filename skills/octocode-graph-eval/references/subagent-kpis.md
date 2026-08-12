# Subagent KPIs
Load when choosing metrics for multi-agent / subagent work. Why: spawn cost is invisible unless measured.

## Why measure workers at all
- Fan-out without a boundary KPI optimizes **busy-ness**, not outcomes.
- Per-node sensors enable attribution; graph-boundary KPI decides ship.
- Guardrails catch Goodhart (cheap parallel spam, latency blowups, permission creep).

## KPI map
| Level | Example | Role |
|---|---|---|
| **Primary (lagging)** | End-to-end case pass rate, task success, user-visible correctness | ACCEPT/REVERT |
| **Leading** | Per-worker case score, exit code, stage latency, packet completeness | Attribution |
| **Guardrails** | Total tokens/time, spawn count, merge conflicts, write-collision count, verifier freshness | Must not regress |
| **Process checks** | Barrier honored, return-shape present, held-out untouched | Fairness |

## Good defaults
```text
Goal: <user-visible outcome of the multi-agent task>
Primary KPI: e2e pass rate (higher-better) baseline=… target=…
Leading: worker_i case score; packet status≠blocked rate
Guardrails: total tokens ≤ budget; spawn count ≤ N; collisions = 0; verifier fresh-context = true
Budget: fixed trials + pinned harness
Held-out: prompts/cases never used to invent topology or packets
Decision: ACCEPT if primary≥target AND guardrails hold else REVERT
```

## Bad KPIs
- “Workers finished” / “felt faster” / star counts
- Grading exact tool-call paths when outcomes suffice
- Optimizing spawn count up (or latency down) with quality collapsing

## Checks before ACCEPT
1. Baseline and result used the **same** sensor command.
2. Held-out still passes.
3. Attribution: blamed node’s own sensor reproduces the failure (or success).
4. No harness edit mid-run.

Next: contract template → `references/kpi-contract.md`; failure modes → `references/graph-failure-modes.md`.
