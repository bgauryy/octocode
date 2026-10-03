# Decompose

Load when you split a goal into worker-sized units or choose a topology. Bad cuts create thrash; good cuts are a DAG.

## Task graph

1. Restate the goal and acceptance in one line.
2. List candidate subtasks as verbs with outputs (`probe X → claim ledger`).
3. Mark edges: `blocks`, `feeds`, `conflicts` (same files, same decision).
4. Tag each node **sync-in-parent** or **async-spawn**. Only async nodes get packets.
5. Collapse work that needs the same evolving context into one parent step.
6. Cap fan-out (default ≤5). Ask before larger swarms.
7. Prefer the smallest plan that can satisfy acceptance.

A subtask runs in parallel only if all hold: inputs are known or cheap to duplicate; no write overlap, or exclusive locks are assigned; one failure does not invalidate another's method mid-flight; the returns merge without another research campaign.

Cut styles: by surface (local, remote, package, web) · by hypothesis (competing explanations) · by layer (data → logic → API; serial if dependent) · by role (research, plan, implement, review) · map-reduce (many similar probes → parent merge).

## Topology (portable across hosts)

| Pattern | When | Do |
|---|---|---|
| ReAct (solo) | Default; one context fits | Parent tools |
| Skills | Progressive disclosure beats a new process | Load `SKILL.md` in the parent |
| Reflexion | The same failure repeats | Parent critique and retry (`references/coordinate.md`) |
| Plan-and-execute | Planning is the bottleneck | Planner worker → parent executes |
| Verifier-critic | Quality is the bottleneck | Second worker with anchors; parent adjudicates |
| Supervisor + specialists | Parallel specialists; manager-as-tool, parent keeps the requester | Spawn workers; parent synthesizes. Production default |
| Handoff | Specialist owns the next turns | Filtered context + return or terminal rule |
| Router | Clear verticals; one classify step | Parent classifies → one specialist |
| Sequential pipeline | Each stage needs the prior artifact | Serial waits |
| Parallel fan-out / scouts | Independent probes or research angles | Spawn all → `references/completion.md` barrier |
| Hierarchical | Deeper cuts | Parent fans out; no nested spawn by default |
| Swarm | Exploratory peer routing | Avoid for production coding |
| A2A collective / challenge / local Ollama | Remote peers / second mind / tool-less text | Routes in `SKILL.md` |
| Bounded improve | Harness KPIs, never unbounded self-modification | `octocode-eval-benchmark` |

- Supervisor is multi-turn; router is one classify step.
- Sync: parent tools. Async: spawn, then wait or poll status.

Next: write packets with `references/packets.md`.
