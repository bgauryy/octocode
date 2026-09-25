# Exposed vendor message APIs

Checked September 25, 2026. The shared SQLite store remains the cross-vendor
transport. Native APIs are optional ways to deliver a stored message into a
particular running agent; they do not replace our identity, retention, leases,
recipient snapshots, retry keys or acknowledgements.

## Codex

The installed App Server schema and [official reference](https://learn.chatgpt.com/docs/app-server)
expose three useful operations:

| Operation | Behavior |
| --- | --- |
| `thread/inject_items` | Append raw Responses API items to a loaded thread's model-visible history without starting a turn. |
| `turn/start` | Deliver input and start generation. |
| `turn/steer` | Add input to an active turn, guarded by `expectedTurnId`; fails if no turn is active. |

Live test on Codex 0.155.0-alpha.9.2: inject a user-role peer notification with a
unique receipt, observe zero turns, explicitly start one Luna turn, and check that
the response recalls that receipt. Passed. [Receipt](../out/codex-inject-api-check.json).
The receipt was not repeated in the later prompt. No tools were called.

Use peer attribution in content. Do not inject peer text as developer/system
authority or invent an assistant answer. This targets a thread loaded by the
connected server; it is not proof of access to arbitrary sessions in other hosts.
For idle notifications requiring action, `turn/start` already fits our current
adapter. Injection is useful for passive context; it does not wake the model.

## Claude

[Cross-session messaging](https://code.claude.com/docs/en/cross-session-messaging)
exposes model tools `ListAgents` and `SendMessage` between independent sessions.
It also documents `CLAUDE_CODE_MESSAGING_SOCKET` and an authentication token for
scripts/hooks. Bare mode does not bind that inbox. Inbound controls can accept,
hold or refuse a message; print-mode workers can receive messages.

Live test on Claude Code 2.1.281: two disposable Haiku processes, empty MCP config,
disabled skill commands, process-local inbound acceptance, one sender with only
`SendMessage` and a receiver with no tools. The sender addressed only the exact
test receiver. Its native send succeeded; the receiver started another turn and
echoed the unique receipt. Passed. [Receipt](../out/claude-peer-api-check.json).
That test validated the model tool. A subsequent direct socket test, below,
validates delivery without a sender model. Authentication/config files were
unchanged; the disposable processes were stopped.

Other supported entry points:

- [Streaming SDK input](https://code.claude.com/docs/en/agent-sdk/streaming-vs-single-mode)
  accepts a sequence of user messages. Our current CLI adapter uses stream-json
  input for the same owned-process workflow.
- [MCP channels](https://code.claude.com/docs/en/channels-reference) advertise
  `experimental['claude/channel']` and emit `notifications/claude/channel` with
  content and optional metadata. This is an external push interface, with preview
  availability, authentication and organization-policy constraints. Custom channels
  require explicit enablement; this audit did not run a channel server.

## Pi and generic agents

Pi's native extension/SDK injection was [verified separately](PI_MESSAGES.md).
None of these vendor capabilities are required to use the communication skill.
The default contract is:

```text
any agent -> bound tools / Rust CLI / compatible SQLite client -> shared store
shared store -> recipient polling or optional vendor adapter -> agent context
agent handles message -> explicit acknowledgement
```

Agents without skill discovery can read the single `SKILL.md` directly. Agents
without vendor APIs can use the bundled CLI. If that executable cannot run, a
coordinator supplies an initialized DB and the exported `db protocol`/`schema`;
the agent uses a conforming SQLite client. The reference Python client needs a
compatible SQLite version, and Unicode 16 for locks. Access to the same local
store and canonical workspace is required; remote-only agents need a local bridge.

Fresh validation after the skill update: all 18 native/process tests passed,
including the standalone copied skill without vendor CLIs, Node or Cargo on PATH,
and direct SQLite/Python interop for bidirectional messages, broadcasts, acks and
lock conflicts. The skill is one 49-line instruction file. The embedded skill
and standalone archive were rebuilt; the standard skill validator passed.

Native delivery does not imply processing or acknowledgement. Keep DB delivery
IDs, sender attribution and retry handling across every adapter. Do not write
vendor transcript files or private mailbox formats to simulate receipt. Current
production adapters retain their tested idle-turn delivery paths; this API audit
does not claim the newly examined alternatives have been integrated.

## Direct transport: no sender agent

The opt-in [probe](../scripts/direct-transport-poc.mjs) creates only disposable
receivers and addresses only their explicitly selected endpoints. Its
[latest receipt](../out/direct-transport-poc.json) passed with Claude Code 2.1.282
and Codex 0.155.0-alpha.9.2. Run it with `node scripts/direct-transport-poc.mjs`.
It uses the installed authenticated CLIs and incurs recipient inference costs.

| Path | Observed transport time | Sender inference | Recipient behavior |
| --- | --- | --- | --- |
| Node Unix socket → Claude inbox | 8.9 ms | None | Idle receiver generated one response, completing in 2.43 s. |
| Separate WebSocket client → Codex `thread/inject_items` | 337 ms | None | No turn or usage event during a 1.5 s observation; a later explicit turn recalled the marker. |

These are individual local observations, not latency guarantees. Codex timing
includes the probe's 25 ms response polling granularity and first injection setup.
The Codex sender client created zero threads. Both recipients used empty temporary
working directories, disabled tools/skills/MCP as applicable, and Haiku/Luna. All
owned child processes were reaped and the temporary directory removed.

### What Claude does internally

The installed runtime's own debug guidance supplies the socket envelope:

```json
{"type":"user","message":{"role":"user","content":"message text"}}
```

The inspected 2.1.282 implementation reads newline-delimited JSON, validates the
optional `session_id`, checks inbound policy, attaches peer provenance, skips slash
commands and attachment expansion, and queues the text for the existing receiver.
The test supplies the receiver's `session_id`, `from`, `uuid` and `msg_id`; a wrong
session ID produced no response. The exact same frame, including UUID, produced
no second completed turn during an eight-second observation. This is a bounded
observation, not a durable exactly-once contract across restarts or versions.

The socket and auth mechanism are documented; the complete envelope was confirmed
from this installed version and live execution. Treat it as a version-sensitive
adapter. No auth token was read or copied: same-user Unix socket delivery was
accepted under this disposable receiver's explicit inbound policy. Windows auth
and held/refused policies were not tested here. Socket close is not a processing
acknowledgement. A real adapter must keep our DB acknowledgement separate.

### What Codex does internally

At upstream revision `d7b07d45517a793acfba4cbf8de697d723cceb46`, app-server's
[injection handler](https://github.com/openai/codex/blob/d7b07d45517a793acfba4cbf8de697d723cceb46/codex-rs/app-server/src/request_processors/turn_processor.rs#L974) loads
the thread, checks direct-input access, validates each Responses API item, and
calls [`CodexThread::inject_response_items`](https://github.com/openai/codex/blob/d7b07d45517a793acfba4cbf8de697d723cceb46/codex-rs/core/src/codex_thread.rs#L738). The core records history and checkpoints
without running a model turn. The native [`send_input` handler](https://github.com/openai/codex/blob/d7b07d45517a793acfba4cbf8de697d723cceb46/codex-rs/core/src/tools/handlers/multi_agents/send_input.rs)
instead routes through local agent control; it can interrupt/resume work and is
not a universal external mailbox. App-server is the exposed integration surface.

The live test used two ordinary clients connected to one owned server: one owns
the receiver thread; the other injects attributed peer data without starting a
sender thread. It does not establish access to an arbitrary desktop or CLI session:
that session must be reachable through its owning server and allow direct input.
Opening another server against the same saved transcript is not equivalent.

The first Unix `app-server proxy --sock` experiment timed out at initialization;
the WebSocket path then passed twice. One intervening recall run failed its exact
model-output assertion; its answer was not captured, so its cause is unresolved.
The probe now records recall output before asserting. This evidence establishes
feasibility, not production reliability of every vendor transport.

### Consequence for our architecture and context

Use a deterministic dispatcher for routing:

```text
sender → shared DB → recipient's existing native endpoint → recipient
```

No LLM relay is needed to copy a message, fan out a topic, or maintain locks.
Existing CLI/DB sends already have zero model calls. The managed `run` command
currently creates a recipient worker; it is not necessary for generic DB use.
A future attached-session adapter should record an explicit endpoint and session
binding, send one attributed delivery, and distinguish injected from handled.
Use DB polling where an existing agent exposes no supported endpoint.

Native injection eliminates an extra relay's conversation, not the recipient's
history. Codex explicitly retains injected items for later model requests; Claude
queues peer messages into its conversation. Prompt caching is not history removal.
Never retry an uncertain write as a new message ID, and never erase an unread
broadcast because another recipient acknowledged it. Keep bounded idempotency
receipts and crash recovery in the DB; native transport alone cannot promise
exactly-once effects.
