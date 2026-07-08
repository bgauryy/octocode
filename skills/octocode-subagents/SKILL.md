---
name: octocode-subagents
description: "Use when Pi work needs background agents: decide, spawn, monitor, message, or synthesize workers through spawnAgent/AgentMessage."
---

# Octocode Subagents

Use this in Pi hosts that expose `spawnAgent` and `AgentMessage`. Skip on hosts without those tools.

## Default Loop

1. **Decide** - spawn only for independent, long-running, adversarial, or parallel hypothesis work.
2. **Prompt** - write a self-contained worker task with scope, evidence anchors, limits, verification, and output format.
3. **Spawn** - choose `resourceMode`, tools, cwd, model, and thinking level based on the worker's job.
4. **Monitor** - list/status/wait workers; steer only when the current turn can be salvaged.
5. **Synthesize** - verify worker claims before using them, reconcile disagreement, and clean up stale workers.

## References

- `references/spawn-decisions.md` - before deciding whether to spawn or stay in the parent.
- `references/api.md` - when setting `spawnAgent` or `AgentMessage` parameters.
- `references/coordination.md` - when writing worker prompts or coordinating multiple workers.
- `references/synthesis.md` - before trusting worker output or finishing with spawned agents.

## Boundary

Use `octocode-awareness` for live workspace locks, signals, verification, and post-outcome reflection. Use this skill only for Pi subagent orchestration.
