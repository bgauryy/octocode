# Graph of Loops
Load when the subject under eval is a multi-agent workflow — a graph whose nodes are agent loops. Why: one end-to-end score cannot tell which node failed.

## Model
A loop is a graph with one node and an edge back to itself. A graph composes loops; every node still needs its own sensor (`feedback-loops.md`). Loop engineering designs the node; graph engineering wires verified nodes — this skill measures both.

## KPI placement
| Level | KPI |
|---|---|
| **Primary (lagging)** | End-to-end outcome at the graph boundary — the user-visible result |
| **Leading** | Per-node sensors: case score, exit code, stage latency |
| **Guardrails** | Per-node budgets + graph-level counter-metrics (total cost, latency, permission scope) |

## Attribution
When the end-to-end primary drops: bisect by node with frozen inputs; grade node **outcomes**, not internal paths; blame a node only after its own sensor reproduces the failure.

## When a loop should become a graph — escalation signals
1. Distinct specialties needing separate context/constraints
2. Parallel fan-out then merge
3. Different model/tools per step
4. Auditable control flow (regulated work)
5. One verifier judging too many dimensions
Rule: **strengthen the verifier before adding a node**; delete nodes that collapse back into the loop.

## Graph-specific failure modes to eval
Opaque state (no typed snapshot) · no checkpoint/resume · unbounded tool permissions per node · missing human gates. Add suite cases for each the first time it appears in a trace.

## Ownership
Topology, spawning, and sealed packets → `octocode-subagent`. This skill owns measuring the graph: contract, sensors, attribution, verdict.

Next: node sensors → `feedback-loops.md`; inner loop per node → `agent-loop.md`; improvement-loop levels → `nested-loops.md`.
