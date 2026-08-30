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
| **Rubber duck** | Need assumption surface cheaply | Listener restates + questions · `rubber-duck.md` |
| **Interview** | Challenge another agent’s claims | Fresh interviewer vs sealed claims · `interview.md` |
| **Mimic flow** | Worker should follow another playbook | Lend filtered instructions · `mimic-flow.md` |
| **Red team / premortem** | Plan too clean; attack or imagine failure | `red-team.md` |
| **Blind review** | Judge artifact without author story | `blind-review.md` |
| **Consensus** | Ambiguous solve; independent retries / majority | `consensus.md` |
| **Subagents / supervisor** | Parallel specialists; **manager-as-tool** — parent keeps user | Spawn workers; parent synthesizes |
| **Handoffs** | Specialist owns next turns | Filtered context + return/terminal rule |
| **Router** | Clear verticals; one-shot classify | Parent classifies → one specialist |
| **Sequential pipeline** | Each stage needs prior artifact | Serial waits |
| **Parallel fan-out** | Independent probes | Spawn all → `synthesize.md` barrier |
| **Hierarchical** | Deeper cuts | Parent fans; avoid nested spawn by default |
| **Swarm** | Exploratory peer routing | Avoid for production coding |
| **A2A collective** | Remote independent agents | `a2a.md` |
| **Bounded improve** | Harness KPIs — not unbounded RSI | `improve-loop.md` |
| **Local Ollama offload** | Save tokens; tool-less summarize/extract/… | `local-ollama.md` |

## Notes
- Supervisor ≠ router: supervisor is multi-turn; router is one classify step.
- Sync = parent tools; async = spawn + wait/status.
- Default production: **supervisor + specialists**.
- Challenge techniques → `references/techniques.md`. Local token burn → `references/local-ollama.md`. Measure → `octocode-eval-benchmark`.

Next: `references/packets.md` · `references/coordinate.md` · `references/synthesize.md` · `references/techniques.md` · `references/local-ollama.md`.
