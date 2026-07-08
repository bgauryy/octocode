---
name: octocode-agent-communication
description: "Use when older prompts name octocode-agent-communication. Route message work to octocode-awareness."
---

# Octocode Agent Communication

Compatibility routing skill for older prompts. Load `octocode-awareness` for message work.

## Route

1. Switch to `octocode-awareness`.
2. Use its `signal publish|list|reply|ack|resolve`, `agent register|list`, and `schema json-schema agent_signal` command families.
3. Keep this folder script-free; operational logic belongs to the primary awareness skill. Do not add operational script logic here.

## References

- `references/compatibility.md` — when explaining why this stub exists or how message commands route.
