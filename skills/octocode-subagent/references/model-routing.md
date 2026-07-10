# Model Routing

Load when choosing `model` / `thinking` per worker. Why: pay for the intelligence the subtask needs.

## Live table first
Use `pi -ne --list-models [search]` (or host equivalent). Map names from the **configured** table — never invent providers.

## Three tiers (keep it operable)

| Tier | Assign when | Examples |
|---|---|---|
| **Small / fast** | Bounded lookup, classify, format, single-surface search, status parse | researcher probes, routers |
| **Balanced** | Ordinary coding/reasoning, multi-file but low-risk plans | planner, most spawnAgent jobs |
| **Strong** | Architecture, security, migrations, root-cause, high-risk multi-file | architect, contested synthesis |

Default thinking: typed registry defaults (`researcher`/`planner` low; `architect` medium) unless the packet raises risk.

## Route vs cascade
- **Route** — pick tier before spawn (preferred for interactive agents).
- **Cascade** — escalate only if acceptance fails or confidence stays uncertain after one replan — not on every turn.

IF the task is a simple one-shot and parent already holds a capable model THEN skip spawn entirely — see `spawn-gate.md`.
IF quality collapses on the small tier THEN escalate once with a tighter packet, not a larger swarm.

## Least privilege
Smallest model that **reliably** meets acceptance. A 70% small model that always cascades wastes latency.

Next: failures in `recovery.md`; remote agents in `a2a.md`.
