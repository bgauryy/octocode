---
name: octocode-subagent
description: "Use when a task may benefit from delegation or parallelism: decide whether to spawn, decompose work into bounded objectives, choose worker topology and model capability, create self-contained handoffs, coordinate shared state, recover failures, and synthesize trustworthy results."
---

# Octocode Subagent

Host-agnostic delegation for local workers or independent remote agents. Flow: `GATE → DECOMPOSE → ROUTE → PACKET → SPAWN → COORDINATE → SYNTHESIZE → CLEANUP`.

## Lobby rules
1. Spawn only when delegation changes speed, expertise, isolation, or context quality; otherwise keep work in the parent.
2. One bounded objective per worker; no nested spawning unless the host explicitly allows it.
3. Workers inherit no parent chat: every packet carries goal, scope, context, authority, constraints, evidence needs, and return shape.
4. Treat worker output as claims; re-check load-bearing anchors.
5. Barrier before synthesize — wait/list every live worker (or stop+remove); merge conflicts first; then answer.
6. Parent owns the user, synthesis, and mutations unless a packet explicitly transfers write ownership.
7. Pick the smallest capable configured model; declare file ownership before parallel writes.
Stop when solo work finishes, two High options need a winner, three angles add nothing, a user/auth gate is pending, or no live workers remain.

## Smart routes — load only what the current step needs
- When deciding solo, batch, specialist, or clean worker, load `references/spawn-gate.md` — delegation must earn its coordination cost.
- When splitting work, load `references/decompose.md`; when choosing supervisor, pipeline, handoff, or swarm load `references/patterns.md` — create a dependency-aware topology.
- Before spawning, load `references/packets.md`; when delegating technical research load `references/octocode.md` — make worker context and tool routing self-contained.
- When selecting model/thinking effort, load `references/model-routing.md` — match capability and cost to objective difficulty.
- When waiting, steering, messaging, or stopping workers, load `references/coordinate.md`; for independent remote peers load `references/a2a.md` — use the correct lifecycle contract.
- When parallel writers share mutable state, load `references/workspace.md` — assign ownership and prevent collisions.
- When workers stall, fail, or conflict, load `references/recovery.md`; before final output load `references/synthesize.md` — reconcile claims, gaps, and live workers.
- When grounding orchestration guidance in sources, load `references/references.md` — preserve provenance.
- When improving this skill, prefer `octocode-eval`; otherwise load `references/improve-loop.md` — require measurable acceptance.

## Related routes
- Use `octocode-research` for worker evidence; `octocode-awareness` for shared-repo coordination; `octocode-eval` to judge worker quality.
- Use `octocode-rfc-generator` before changing a multi-agent architecture; `octocode-prompt-optimizer` for packet contracts; `octocode-skills` when changing this folder.
