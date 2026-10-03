# Spawn Gate

Load before spawning: choose solo, batch, or worker, then worker kind and model tier. Activate only on an explicit substantial delegation request, or a consequential task with at least two independently useful workstreams beyond batchable reads. Agent wording never overrides this value/cost gate.

| Situation | Do |
|---|---|
| Dependent steps, shared context, ordinary edits or synthesis | Stay in the **parent** |
| Independent tool calls, known inputs | **Batch** in one turn |
| A skill or prompt pack covers the job | Load it in the parent; no spawn |
| Low-risk summarize, extract, classify on saved text | **Local Ollama**: `references/local-ollama.md` |
| Named specialist role (research, plan, review) | **Typed specialist** via the host API |
| Purpose-built objective, custom tools and brief | **Clean worker**, minimal tools |
| Independent remote peer | **A2A**: `references/coordinate.md` |
| Specialist must own the next user turns | **Handoff** packet |

## Host model tier
Map tiers to the host's configured catalog (CLI list, settings, provider table); never invent providers.

| Tier | Assign when |
|---|---|
| Small / fast | Bounded lookup, classify, format, single-surface search (probes, routers) |
| Balanced | Ordinary coding or reasoning; multi-file, low-risk (planners, most workers) |
| Strong | Architecture, security, migrations, root cause, high-risk multi-file, contested synthesis |

- **Route** (preferred, interactive): pick the tier before spawn.
- **Cascade**: escalate once, with a tighter packet, not a larger swarm, only when acceptance fails or confidence stays uncertain after one replan.
- Use the smallest model that reliably meets acceptance; a model that always cascades wastes latency.

```text
Decision: SOLO | BATCH | SPAWN | HANDOFF
Why: <speed | expertise | isolation | context>
Workers: <n> · Topology: <pattern> · Packets sealed?: yes/no · Ownership declared?: yes/no
Authority/budget bounded?: yes/no · Technique (optional): see references/challenge.md
```

Next: split with `references/decompose.md`; brief with `references/packets.md`.
