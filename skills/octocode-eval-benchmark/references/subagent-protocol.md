# Subagent Protocol
Load when running an evaluated multi-agent iteration. Why: without a fixed protocol, fan-out burns tokens and greenwashes.

## Protocol (frozen during an experiment)
```text
1. FRAME   — goal, primary KPI, guardrails, held-out, decision rule (`references/kpi-contract.md`)
2. TOPOLOGY — edge detection; solo vs graph (`references/graph-of-loops.md`); pick approach (`references/subagent-approaches.md`)
3. PACKET  — one bounded objective per worker; sealed return shape (`octocode-subagent` packets)
4. BASELINE — measure graph-boundary KPI before first spawn/mutation
5. SPAWN   — independent workers only; declare write ownership; spawn all before waiting
6. BARRIER — list/wait/stop leftovers; no synthesize while live (`references/subagent-communication.md`)
7. VERIFY  — parent re-checks anchors; fresh-context verifier if used
8. JUDGE   — ACCEPT/REVERT from comparable results; harness frozen
9. CAPTURE — one durable lesson + failureSignature if recurring
```

## Checks (must run)
| Check | Pass condition |
|---|---|
| Sensor exists | Same command/budget as baseline |
| Edge detection | Real data edges or explicit parallel independence |
| Packet complete | goal, scope, acceptance, return shape present |
| Barrier | Every needed worker idle/terminal before merge |
| Verifier | Fresh context — not executor transcript |
| Anchor | ≥1 non-arguable node outcome recorded |
| Harness | Cases/graders unchanged mid-experiment |

## Stop or revert
- No primary KPI or sensor → stop and define the contract.
- Narrative-only “workers looked good” → revert.
- Edit harness to pass → REVERT
- Primary up + guardrail down → reframe goal (Goodhart), do not keep looping

Next: fill measures → `references/subagent-kpis.md`; communication rules → `references/subagent-communication.md`.
