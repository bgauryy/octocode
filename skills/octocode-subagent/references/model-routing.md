# Model Routing

Load when choosing model / thinking per worker. Why: pay for the intelligence the subtask needs.

## Live table first
Use the **host’s configured model catalog** (CLI list, settings UI, or provider table). Map names from what is actually available — never invent providers.

## Three tiers (keep it operable)

| Tier | Assign when | Examples |
|---|---|---|
| **Small / fast** | Bounded lookup, classify, format, single-surface search | probes, routers |
| **Balanced** | Ordinary coding/reasoning, multi-file but low-risk | planners, most workers |
| **Strong** | Architecture, security, migrations, root-cause, high-risk multi-file | architects, contested synthesis |

## Route vs cascade
- **Route** — pick tier before spawn (preferred for interactive agents).
- **Cascade** — escalate only if acceptance fails or confidence stays uncertain after one replan.

IF the task is a simple one-shot and parent already holds a capable model THEN skip spawn entirely — see `spawn-gate.md`.
IF quality collapses on the small tier THEN escalate once with a tighter packet, not a larger swarm.

## Least privilege
Smallest model that **reliably** meets acceptance. A weak small model that always cascades wastes latency.

Next: `recovery.md` · `a2a.md`.
