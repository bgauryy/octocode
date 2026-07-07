# Hook Delivery

Load this when using hook-delivered inbox context, installing hooks, or explaining what hooks do in communication.

## Boundary

Hooks are delivery and reminder surfaces. The database is the broker.

Hooks may:

- Surface unread signals on the next prompt or lifecycle event.
- Remind the agent to ack messages after acting.
- Remind the agent to resolve a thread before finishing.
- Fail open when a host event payload is missing or the DB is unavailable.

Hooks should not:

- Store messages outside the awareness DB.
- Ack unread messages implicitly.
- Resolve threads automatically.
- Run network protocol work or model calls.
- Become a public A2A server.

## Host Notes

Awareness hooks may surface inbox context before this skill is loaded. When `octocode-awareness` says messages are available, load this skill before acting so replies, acknowledgements, resolution, and A2A mapping stay consistent.

Codex does not execute standalone `SKILL.md` hook frontmatter. Codex hook wiring must come from project `.codex` config, user config, or plugin hook config. Use `octocode-awareness` hook installation guidance before changing hook config.

Claude-style hosts may read hook frontmatter, but communication should still go through `scripts/awareness.mjs agent-signal`.

Pi hosts can register the current agent identity during startup and surface signals through the awareness bridge.

## Delivery Semantics

Unread delivery is intentionally repeatable. Seeing a message in hook context does not mark it read.

The receiver should:

1. Inspect the signal.
2. Do the requested or appropriate action.
3. Reply if needed.
4. Ack the specific signal ids.
5. Resolve the thread only when the conversation is complete.

Repeatable delivery keeps hook-delivered messages safe across interrupted runs and context compaction.
