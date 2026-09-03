# Graph of Loops
Load when the subject under eval is a multi-agent workflow — a graph whose nodes are agent loops. Why: one end-to-end score cannot tell which node failed.

## Model
A loop is a graph with one node and an edge back to itself. A graph composes loops; every node still needs its own sensor (`feedback-loops.md`). Loop engineering designs the node; graph engineering wires verified nodes — this skill measures both.

## KPI placement
| Level | KPI |
|---|---|
| **Primary (lagging)** | End-to-end outcome at the graph boundary — you-visible result |
| **Leading** | Per-node sensors: case score, exit code, stage latency |
| **Guardrails** | Per-node budgets + graph-level counter-metrics (total cost, latency, permission scope) |

## Attribution
When the end-to-end primary drops: bisect by node with frozen inputs; grade node **outcomes**, not internal paths; blame a node only after its own sensor reproduces the failure.

## Edge detection — real vs fake dependencies
Before measuring a graph, verify its topology is real. For every "and then" in the workflow:
- Does the next step **read** the previous step's output?
  - **Yes** → real edge; ordering is required. <!-- style-lint: ignore-line passive-voice -->
  - **No** → no real edge; the wait is wasted; steps can run in parallel. <!-- style-lint: ignore-line passive-voice -->

A graph whose steps all wait for prior steps with no data crossing between them is a line masquerading as a graph. It adds coordination cost with zero throughput gain. Measure fake graphs by removing sequential waits and timing the result.

## When a loop must become a graph — escalation signals
1. Distinct specialties needing separate context/constraints
2. Parallel fan-out then merge
3. Different model/tools per step
4. Auditable control flow (regulated work)
5. One verifier judging too many dimensions
Rule: **strengthen the verifier before adding a node**; delete nodes that collapse back into the loop.

## When NOT to use a graph
- Task is small or isolated — one function, one bug; a graph is pure overhead.
- Oversight required at every step — a graph's whole value is running wide without you; that works against tight approval loops.
- Exploratory / unknown shape — one steerable agent beats a fleet committed to a plan you don't understand yet.
- Steps genuinely depend on each other — if every step reads the last step's output, there's no parallel width to exploit. Forcing a graph onto a truly sequential task adds coordination cost for zero speedup.

Test: if edge detection (above) finds no two boxes with no arrow between them, build a loop, not a graph.

## Graph-specific failure modes
Load `references/graph-failure-modes.md` — shared-context verifier, race conditions, Goodhart's Law, missing anchors. Add suite cases on first trace appearance.

## Ownership
Topology, spawning, and sealed packets → `octocode-subagent`. This skill owns measuring the graph: contract, sensors, attribution, verdict — start at `subagent-cookbook.md` for protocol/KPI/comms cookbooks.

Next: node sensors → `feedback-loops.md`; inner loop per node → `agent-loop.md`; improvement-loop levels → `nested-loops.md`; bilevel escalation → `karpathy-patterns.md`; subagent measurement → `subagent-cookbook.md`.
