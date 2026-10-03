# Spawn Gate

Load before spawning: decide solo, batch, or worker; pick the worker kind and model tier. Activate only for an explicit substantial delegation request, or a consequential task with at least two independently useful workstreams beyond batchable reads. Explicit agent wording never overrides this value/cost gate.

## Decide and pick the kind

| Situation | Do |
|---|---|
| Dependent steps, shared context, ordinary edits or synthesis | Stay in the **parent** |
| Independent tool calls with known inputs | **Batch** in one turn |
| A skill or prompt pack covers the job | Load the skill in the parent; do not spawn |
| Low-risk summarize, extract, classify on saved text; save tokens | **Local Ollama**: `references/local-ollama.md` |
| Named specialist role (research, plan, review) | Delegate a **typed specialist** through the host API |
| Purpose-built objective with custom tools and brief | Spawn a **clean worker** with minimal tools |
| Independent remote peer | **A2A**: `references/coordinate.md` |
| Specialist must own the next user turns | **Handoff** packet: filtered history plus a return rule |

- If the parent, a skill, or one batch finishes cheaply, do not spawn.
- If subtasks need each other's live context, keep them serial in the parent.
- If workers are independent, spawn all before waiting on any.
- If approval is pending, authority is missing, or every step consumes the prior evolving result, stop or stay serial in the parent.

## Host model tier

Map names from the host's configured model catalog (CLI list, settings, provider table). Never invent providers.
| Tier | Assign when |
|---|---|
| Small / fast | Bounded lookup, classify, format, single-surface search (probes, routers) |
| Balanced | Ordinary coding or reasoning; multi-file but low-risk (planners, most workers) |
| Strong | Architecture, security, migrations, root cause, high-risk multi-file, contested synthesis |

- **Route** (preferred for interactive agents): pick the tier before spawn. **Cascade**: escalate once, with a tighter packet and not a larger swarm, only when acceptance fails or confidence stays uncertain after one replan.
- Use the smallest model that reliably meets acceptance. A small model that always cascades wastes latency.

## Decision card

```text
Decision: SOLO | BATCH | SPAWN | HANDOFF
Why: <speed | expertise | isolation | context>
Workers: <n> · Topology: <pattern> · Packets sealed?: yes/no · Ownership declared?: yes/no
Authority/budget bounded?: yes/no · Technique (optional): see references/challenge.md
```

Anti-patterns: a spawn for one read or search; ceremonial fan-out for cheap reads, tiny edits, or deterministic work; parallel writers on one path without ownership; idle or "phase done" taken as acceptance; recursive workers the host does not document.

Next: split the work with `references/decompose.md`; brief with `references/packets.md`.
