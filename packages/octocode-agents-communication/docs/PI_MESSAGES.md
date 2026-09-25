# Pi native message injection

Verified September 24, 2026 against installed Pi 0.87.1 and upstream API declarations.

Pi extensions expose `pi.sendMessage(message, options)`. The SDK equivalent is
`await session.sendCustomMessage(message, options)`. Both create a custom message
in the target Pi session; neither provides cross-process recipient lookup or transport.

```js
pi.sendMessage({
  customType: 'agent-message',
  content: 'Peer claude-1 reports: lease released.',
  display: true,
  details: { from: 'claude-1', messageId: 'db-message-id' },
}, { triggerTurn: true, deliverAs: 'followUp' });
```

- Idle + `triggerTurn: false`: append to session/context without a model call.
- Idle + `triggerTurn: true`: start a model turn.
- Running + `deliverAs: 'followUp'`: queue for follow-up processing. Default
  delivery steers instead. Explicit `triggerTurn: false` defers the append until
  the current turn ends without requesting continuation.
- `deliverAs: 'nextTurn'`: retain pending context until the next user prompt;
  this branch takes precedence over `triggerTurn` and does not append immediately.
- `display: false` hides transcript rendering; it does **not** exclude model context.
- `pi.appendEntry()` stores extension state without adding model context.
- `ctx.ui.notify()` produces a UI notification, not a conversation message.
- `pi.sendUserMessage()` adds a user message and triggers processing; it supports
  steering/follow-up while running.

Custom messages retain `role: 'custom'`, type and details inside Pi. The default
provider conversion maps their content to a user-role message and omits custom
metadata. Put sender/provenance in content when the model needs it; metadata alone
is insufficient. Keep peer content distinct from authorized user instructions.

## Live check

A temporary explicit extension was loaded with skills, other extension discovery,
context files and tools disabled. An RPC slash command called `pi.sendMessage()`
twice with `triggerTurn: false`, once displayed and once hidden. RPC `get_messages`
returned both custom messages with sender/ID metadata. Conversion through Pi's
installed `convertToLlm` included both contents. A separate custom state entry
appeared in `get_entries` but not messages; a UI notification was emitted separately.
No `agent_start` event or model call occurred. All assertions passed.

[Captured result](../out/pi-message-api-check.json). Streaming scheduling and
model-triggering behavior above were traced in installed implementation, not
exercised by this zero-model-call probe.

## Communication adapter implication

The current Rust adapter sends incoming deliveries using RPC `prompt`; its Pi
extension only registers the nine bound coordination tools. The installed RPC
command union exposes `prompt`, `steer` and `follow_up`, but no direct custom-message
injection command. Use an explicit extension or SDK adapter to call native
`sendMessage`; do not invent a `send_message` RPC command.

The existing SQLite transport can remain responsible for recipient routing,
durability, claims and acknowledgement. A Pi extension can receive a validated DB
delivery and inject it into its own session using the native API. Broadcast still
requires a delivery per recipient. Successfully appending a message does not prove
that the receiving agent processed or acknowledged it.

Sources: [extension API](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/extensions/types.ts),
[session implementation](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/agent-session.ts),
[RPC commands](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/rpc-commands.md).
