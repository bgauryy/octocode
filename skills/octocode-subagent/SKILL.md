---
name: octocode-subagent
description: "Use when work needs task breakdown, subagent delegation, parallel workers, model routing by difficulty, handoffs, A2A remote agents, or supervisor/orchestrator patterns."
---

# Octocode Subagent

Host-agnostic meta-skill for decomposing work and running specialists. Parent owns the user, synthesis, and mutations unless a packet explicitly transfers write ownership. Map actions to whatever spawn/Task/teammate API the host provides.

Flow: `GATE → DECOMPOSE → ROUTE → PACKET → SPAWN → COORDINATE → SYNTHESIZE → CLEANUP`.

## Hard rules
1. Prefer parent, loaded skill, or batched tool calls when one context finishes cheaply.
2. One bounded objective per worker; no nested spawning unless the host explicitly allows it.
3. Workers inherit no parent chat — packet must be self-contained.
4. Treat worker output as claims; re-check load-bearing anchors.
5. Barrier before synthesize — wait/list every live worker (or stop+remove); merge conflicts first; then answer.
6. Pick the smallest capable model from the host's configured model table.
7. Shared workspaces are mutable — declare file ownership before parallel writes.

## Stop when
Solo finishes; two High options need a winner; three angles add nothing; a user/auth gate is pending; or no live workers remain to reconcile.

## Reference map
- `references/spawn-gate.md` — when choosing solo, batch, specialist, or clean worker.
- `references/decompose.md` — when splitting a goal into a dependency DAG.
- `references/patterns.md` — when picking supervisor, handoff, pipeline, or swarm topology.
- `references/packets.md` — when writing request/result briefs for workers.
- `references/coordinate.md` — when waiting, steering, messaging, or stopping workers.
- `references/model-routing.md` — when mapping task difficulty to model tier.
- `references/a2a.md` — when calling an independent remote agent peer.
- `references/synthesize.md` — when merging worker results before the final answer.
- `references/recovery.md` — when workers fail, stall, or conflict.
- `references/workspace.md` — when parallel writers share a repo or cwd.
- `references/improve-loop.md` — when improving this skill with measurable KPIs.
- `references/octocode.md` — when workers need Octocode research tool routing.

## Next skills
`research` · `awareness` · `rfc-generator` · `prompt-optimizer` (agent-communication).
