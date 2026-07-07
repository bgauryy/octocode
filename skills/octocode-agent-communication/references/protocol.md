# Communication Protocol

Load this when sending, reading, replying to, acknowledging, resolving, or handing off messages.

## Store

Communication uses the same awareness SQLite database as memory, locks, sessions, refinements, and reflection. Do not create a second message store.

| Layer | Table/API | Purpose |
|---|---|---|
| Identity | `agents`, `agent-registry` | Known `agent_id`, display name, host context, last-seen scope |
| Message | `signals`, `agent-signal` | Typed messages, recipients, threads, refs, related files |
| Read receipt | `signal_reads`, `agent-signal --action ack` | Idempotent per-agent acknowledgement |

## Mailbox Model

The inbox is open unread signals for the current `agent_id`, plus broadcasts where `to_agent` is `NULL`.
The outbox is signals where `from_agent` is the current agent.
Use `thread_id` as the thread/topic instead of inventing a separate topic store.

A message reaches an agent that has not loaded this skill only when an awareness surface runs.
Those surfaces are hooks, Pi bridge, manual `octocode-awareness` polling, or this skill's inbox command.
Without one of those surfaces, the message stays durable in SQLite but cannot be pushed into model context.

## Identity

Register the current agent when the host has not already done it:

```bash
node scripts/awareness.mjs agent-registry --action register \
  --agent-id codex-2 \
  --agent-name "Codex repo worker" \
  --workspace /repo \
  --artifact packages/octocode-awareness \
  --context codex
```

List likely recipients before targeted delivery:

```bash
node scripts/awareness.mjs agent-registry --action list \
  --workspace /repo \
  --artifact packages/octocode-awareness \
  --compact
```

If the receiver is unknown, broadcast by omitting `--to-agent`.

## Send

Use typed signals so recipients can filter and act without parsing a long prose note.

```bash
node scripts/awareness.mjs agent-signal --action publish \
  --agent-id codex-2 \
  --to-agent claude-1 \
  --kind question \
  --subject "Can you review the handoff?" \
  --body "Please check the lock state before editing." \
  --file packages/octocode-awareness/src/intents.ts \
  --ref-id task_123 \
  --importance 7 \
  --workspace /repo
```

Use these kinds deliberately:

| Kind | Use |
|---|---|
| `question` | Ask for information or review |
| `request` | Ask another agent to do a bounded action |
| `blocker` | Warn that work is blocked or unsafe to touch |
| `handoff` | Transfer context or next action |
| `decision` | Broadcast a choice others should know |
| `fyi` | Low-stakes note |
| `claim` | Explain an area or file claim |
| `reply` | Answer in an existing thread |

## Receive

Read unread messages addressed to the current agent or broadcast:

```bash
node scripts/awareness.mjs agent-signal --action list \
  --agent-id claude-1 \
  --workspace /repo \
  --compact
```

Use `--all` when you need already-read messages. Use `--thread-id` to inspect one discussion.

## Reply

Reply with `--in-reply-to`; the parent thread id is inherited.

```bash
node scripts/awareness.mjs agent-signal --action reply \
  --agent-id claude-1 \
  --to-agent codex-2 \
  --subject "Reviewed" \
  --body "The lock is clear; proceed with the docs-only slice." \
  --in-reply-to sig_abc123 \
  --workspace /repo
```

## Ack And Resolve

Ack only after you acted on a message:

```bash
node scripts/awareness.mjs agent-signal --action ack \
  --agent-id claude-1 \
  --signal-id sig_abc123 \
  --workspace /repo
```

Resolve a signal or thread only when no further action is needed:

```bash
node scripts/awareness.mjs agent-signal --action resolve \
  --agent-id codex-2 \
  --thread-id sig_abc123 \
  --workspace /repo
```

## Handoff

For unfinished work, send a `handoff` signal and include related `task_id`, `refinement_id`, or `memory_id` values in `--ref-id`. Durable follow-up still belongs in refinements or memory; messages are the live coordination channel.
