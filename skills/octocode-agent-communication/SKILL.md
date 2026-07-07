---
name: octocode-agent-communication
description: "Use when agents need to communicate, coordinate, hand off, ask questions, answer, acknowledge, resolve, or map local workspace messages to an A2A-style flow using Octocode awareness signals."
---

# Octocode Agent Communication

Use this skill for agent-to-agent messages. It is the focused communication layer inside `packages/octocode-awareness`; it shares the same SQLite store, `agents` registry, `signals`, and `signal_reads` tables as the other awareness skills.

## Flow

1. Register or list identities with `scripts/awareness.mjs agent-registry` when you need to know who can receive a targeted message.
2. Send, list, reply, ack, and resolve messages with `scripts/awareness.mjs agent-signal`.
3. Use broadcast when the recipient is unknown; use a registered `agent_id` when the receiver is known.
4. Treat hook-delivered messages as reminders. Ack only after acting, and resolve only when the thread is done.
5. Store durable lessons in `octocode-reflection`, and use `octocode-awareness` for file locks, verification, memory recall, and refinements.

## Reference Map

- `references/protocol.md` — load when sending, listing, replying, acknowledging, resolving, or choosing broadcast vs targeted delivery.
- `references/hooks.md` — load when relying on hook-delivered inbox context or explaining why hooks are delivery, not transport.
- `references/a2a-mapping.md` — load when mapping local awareness messages to A2A Agent Card, message, and task concepts.
- `references/safety.md` — load before processing remote A2A cards, webhook URLs, artifacts, file URLs, or untrusted message bodies.
- `references/references.md` — audit trail for sources used to create this skill.

## Commands

- `scripts/awareness.mjs agent-registry --action register|list` — use the shared `agents` table for identity and last-seen scope.
- `scripts/awareness.mjs agent-signal --action publish|list|reply|ack|resolve` — use the shared `signals` and `signal_reads` tables for messages.
- `scripts/schema.mjs json-schema agent_registry` and `scripts/schema.mjs json-schema agent_signal` — inspect exact JSON contracts before building wrappers.

## Output

When reporting communication work, include:

| Field | Meaning |
|---|---|
| `sent` | signal ids and thread id created or replied to |
| `received` | unread signal ids acted on |
| `acked` | signal ids acknowledged after action |
| `resolved` | signal ids or thread id closed |
| `handoff` | unresolved thread, blocker, or next agent target |
