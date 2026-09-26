# Pi native message injection

Verified September 24, 2026 against installed Pi 0.87.1 and upstream API declarations.

September 26 repair: delayed ledger flushes can accumulate receipts from several
delivery batches. Pi now discovers `confirm_delivery`'s limit from the CLI catalog
and confirms in bounded chunks, removing only successfully confirmed receipts.
Native staging derives its row cap from that same contract (16), preventing the
previous mismatch with a four-receipt limit. A 35-message regression covers delayed
disk persistence, continued delivery and no replay. Confirmation still proves
native persistence, not recipient handling.

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

## Historical passive-injection check

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

The optional managed Rust worker sends deliveries using RPC `prompt`; its Pi
extension registers thirteen bound coordination tools. The existing-session
`pi-inbox.mjs` adapter uses native custom messages: new action batches set
`triggerTurn:true` at idle, while passive and identity context set it false.
Busy recipients defer until idle. The `before_agent_start` boundary consumes
messages without requesting a nested turn, including when a poll races user input.
The installed RPC
command union exposes `prompt`, `steer` and `follow_up`, but no direct custom-message
injection command. Use an explicit extension or SDK adapter to call native
`sendMessage`; do not invent a `send_message` RPC command.

The SQLite transport remains responsible for recipient routing,
durability, claims and acknowledgement. The extension consumes the Rust CLI’s
canonical `hook.context` string, so message projection and safety guidance have one
renderer. Each new dispatch enters native context once; handling and `ack` remain
agent decisions. Broadcast still
requires a delivery per recipient. Successfully appending a message does not prove
that the receiving agent processed or acknowledged it.

Sources: [extension API](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/extensions/types.ts),
[session implementation](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/agent-session.ts),
[RPC commands](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/rpc-commands.md).

## Durable receipt boundary

September 25 live inspection of Pi 0.87.1 found that a fresh session can expose
custom entries through `getEntries()` and `get_state` while no session file exists.
The SDK defers its first disk write until an assistant message. These observations
prove in-memory context delivery, not restart durability. There is no supported
flush method on the extension's read-only session manager.

The communication inbox adapter confirms a dispatch only after matching its token
in the actual session JSONL file with the expected session header. It keeps
memory-only entries staged while tools/presence remain usable, and checks again at
idle boundaries. It neither fabricates an assistant message nor calls a private
flush method. A fresh session with no persisted identity cannot promise automatic
recipient recovery after process loss; the DB retains messages for explicit recovery.


## Action-wake verification

The current adapter has 19 targeted tests, including idle action versus passive,
busy deferral, a user-turn startup racing an inbox read, fresh-session disk lag,
native delivery failure and reload without duplicate context. These deterministic
tests verify the delivery contract; they do not replace the separate live vendor
matrix. The earlier zero-model probe above remains passive-injection evidence.

`pi.sendMessage` returns immediately; asynchronous native failures surface through
Pi's extension-error channel. Neither starting a turn nor finding a memory entry
is treated as durable acceptance. Missing receipts remain staged and inspectable,
without an automatic repeated wake. Only the stopped lifecycle owner's complete
ledger can justify recovery; application handling still requires explicit `ack`.

## Optional structured-edit lease gate

When registering the adapter, `registerPiInbox(pi, {binary, workspace, database,
requireLeases:true})` blocks Pi's structured `write` and `edit` calls unless the
current bound identity owns a live covering lease. The adapter calls the Rust
`check_write` host command, not a model. Default behavior is unchanged. Shell,
PowerShell, custom tools and later extension input rewrites remain outside this
gate; leases are still advisory globally. See [host lease admission](HOST_LEASE_GUARDS.md)
for a complete setup example, failure behavior, primary sources and test limits.
A single Pi 0.87.1/Haiku live trial also rejected an unleased built-in write before
file creation, then allowed the same write after host lease acquisition; that guide
links the recorded results and provider usage.
