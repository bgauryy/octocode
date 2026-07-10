# Patterns

Load when choosing orchestration topology. Portable across hosts (LangGraph, OpenAI Agents, Cursor Task, Claude teams, …).

## Catalog → action

| Pattern | When | Do |
|---|---|---|
| **ReAct** (solo) | Default; one context fits | Parent tools |
| **Skills** | Progressive disclosure beats a new process | Load `SKILL.md` in parent |
| **Reflexion** | Same failure repeating | Parent critique retry · `recovery.md` |
| **Plan-and-execute** | Planning is the bottleneck | Planner worker → parent executes |
| **Verifier-critic** | Quality is the bottleneck | Second worker; parent adjudicates |
| **Subagents / supervisor** | Parallel specialists; **manager-as-tool** — parent keeps user | Spawn workers; parent synthesizes |
| **Handoffs** | Specialist owns next turns | Filtered context + return/terminal rule |
| **Router** | Clear verticals; one-shot classify | Parent classifies → one specialist |
| **Sequential pipeline** | Each stage needs prior artifact | Serial waits |
| **Parallel fan-out** | Independent probes | Spawn all → `synthesize.md` barrier |
| **Hierarchical** | Deeper cuts | Parent fans; avoid nested spawn by default |
| **Swarm** | Exploratory peer routing | Avoid for production coding |
| **A2A collective** | Remote independent agents | `a2a.md` |
| **Bounded improve** | Harness KPIs — not unbounded RSI | `improve-loop.md` |

## Notes
- Supervisor ≠ router: supervisor is multi-turn; router is one classify step.
- Sync = parent tools; async = spawn + wait/status.
- Default production: **supervisor + specialists**.

Next: `packets.md` · `coordinate.md` · `synthesize.md`.
