# Decompose

Load when you split a goal into worker-sized units or choose a topology. Bad cuts thrash; good cuts form a DAG.

1. Restate goal and acceptance in one line.
2. List subtasks as verbs with outputs (`probe X → claim ledger`).
3. Mark edges: `blocks`, `feeds`, `conflicts` (same files, same decision).
4. Tag each node **sync-in-parent** or **async-spawn**; only async nodes get packets.
5. Cap fan-out (default ≤5); ask before larger swarms. Prefer the smallest plan that meets acceptance.

Run a subtask in parallel only if: inputs are known or cheap to duplicate; writes do not overlap or have exclusive locks; one failure cannot invalidate another's method mid-flight; returns merge without another research campaign.

Cuts: by surface (local, remote, package, web) · hypothesis · layer (data → logic → API; serial if dependent) · role (research, plan, implement, review) · map-reduce (similar probes → parent merge).

| Topology | When | Do |
|---|---|---|
| Reflexion | Same failure repeats | Parent critique and retry (`references/coordinate.md`) |
| Plan-and-execute | Planning is the bottleneck | Planner worker → parent executes |
| Verifier-critic | Quality is the bottleneck | Second worker with anchors; parent adjudicates |
| Supervisor + specialists | Parallel specialists; manager-as-tool | Spawn; parent synthesizes. Production default |
| Router | Clear verticals | One classify step → one specialist (supervisor is multi-turn) |
| Sequential pipeline | Each stage needs the prior artifact | Serial waits |
| Parallel fan-out / scouts | Independent probes or angles | Spawn all → `references/completion.md` barrier |
| Hierarchical | Deeper cuts | Parent fans out each level |
| Swarm | Exploratory peer routing | Avoid in production coding |
| Bounded improve | Harness KPIs; never unbounded recursive self-modification of models, weights, or policies | `octocode-eval-benchmark` |

Next: write packets with `references/packets.md`.
