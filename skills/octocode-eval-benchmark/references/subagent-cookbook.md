# Subagent Cookbook
Load when measuring or managing a multi-agent / subagent workflow under this skill. Why: spawn mechanics live in `octocode-subagent`; this cookbook owns **why**, **KPIs**, **checks**, and **communication contracts** so keep/discard is honest.

## Ownership split
| Concern | Owner |
|---|---|
| Spawn gate, packets, coordinate, synthesize, topology catalog | `octocode-subagent` |
| Goal→KPI, sensors, held-out, Goodhart, verifier independence, ACCEPT/REVERT | **this skill** |
| Packet wording polish | `octocode-prompt-optimizer` after KPI is fixed |

## Load next (one at a time)
| Need | Ref |
|---|---|
| Protocol for an evaluated multi-agent run | `references/subagent-protocol.md` |
| KPIs / why / what to check | `references/subagent-kpis.md` |
| Communication + barrier contracts | `references/subagent-communication.md` |
| Common topologies + best approaches | `references/subagent-approaches.md` |
| Edge detection / attribution | `references/graph-of-loops.md` |
| Shared-context / races / anchors | `references/graph-failure-modes.md` |

## Hard gate before fan-out
1. Runnable sensor + numeric primary at the **graph boundary**.
2. Edge detection passes (`references/graph-of-loops.md`) — else build a loop, not a graph.
3. At least one **anchor** node (tests/build/types) — no narrative-only graphs.
4. Packets sealed; workers treated as claims until parent re-checks.

Next: start with `references/subagent-protocol.md` unless you already have a filled KPI contract.
