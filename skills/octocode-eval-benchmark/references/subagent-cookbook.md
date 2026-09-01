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

Before fan-out, use `references/subagent-protocol.md` to freeze the sensor, graph-boundary KPI, topology check, anchor, and packet/barrier rules. This file only routes the measurement concerns.

Next: start with `references/subagent-protocol.md` unless you already have a filled KPI contract.
