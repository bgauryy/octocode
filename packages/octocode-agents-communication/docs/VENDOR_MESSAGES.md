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
vendor transcript files or private mailbox formats to simulate receipt. The production Rust CLI now implements these attached-session transports; the
managed worker adapter remains optional.

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
The attached-session adapter records an explicit endpoint and session binding,
sends one attributed delivery, and distinguishes submitted from handled. Use the
bundled hook where a host exposes no native endpoint, or manual CLI/DB inbox reads.

Native injection eliminates an extra relay's conversation, not the recipient's
history. Codex explicitly retains injected items for later model requests; Claude
queues peer messages into its conversation. Prompt caching is not history removal.
Never retry an uncertain write as a new message ID, and never erase an unread
broadcast because another recipient acknowledged it. Keep bounded idempotency
receipts and crash recovery in the DB; native transport alone cannot promise
exactly-once effects.

## Production DB-first adapters

Implemented in `rust/dispatch.rs` and `rust/transport.rs`; Pi's existing-session
bridge is `skills/octocode-agents-communication/scripts/pi-inbox.mjs`. These read
committed DB deliveries and record durable attempt/confirmation state before and
after native injection. All participants join and all replies return through the DB.
The skill is 49 lines; command help owns the detailed schemas. No relay model or
private transcript mutation is involved. Generic `scripts/inbox-hook` emits new
context for an explicit host event; no host API is required for manual CLI use.

Reproduce with `node scripts/attached-poc.mjs` after building. The
[latest machine-readable report](../out/attached-poc.json) records the actual build,
latency and evidence DB location. The probe checks one real recipient per native
vendor plus a raw agent: request/reply, handling acknowledgements, broadcast to all
four recipients, no repeated hook delivery, zero sender model calls, passive
Codex/Pi injection until an explicit prompt, and owned-process cleanup. The retained
isolated SQLite fixture contains all five identities and nine messages (four
requests, four replies, one broadcast), twelve recipient rows and dispatch/audit
receipts. The controller's four reply deliveries need not be acknowledged to prove
recipient handling. This is a local acceptance test, not a reliability or scale SLO.

An earlier run exposed timing ambiguity between hook stdout and Pi queueing, plus
a probe that could prompt before Pi settled. Pi now stages the hook, queues native
context, then confirms the token; the probe waits for `agent_settled`. Unconfirmed
attempts never replay automatically. Explicit recovery may duplicate a message
whose host receipt was lost. Per-vendor history and real recipient inference costs
remain; missing host usage is reported as unknown rather than zero.

### Latest optimized local run

September 25 acceptance passed: Claude socket submission **9.8 ms**, Codex passive
injection **354.6 ms**, raw CLI hook/reply/ack **40.3 ms**. Full request-to-handled
times were Claude **3.65 s**, Codex **27.96 s**, Pi **3.01 s**; model/provider latency
is separate from transport. These are single observations, not benchmarks. Both
persistent native listeners delivered the broadcast. All owned children were reaped.

The DB contains 13 actual usage reports. Pi's four request contexts were
3,079–3,585 tokens. Codex's latest request context was 8,077 tokens; its final
cumulative input was 46,950 with 38,400 cached input tokens across both turns.
Claude's three result reports are aggregate turn scopes, not single-request context
sizes. Cache read/write counters are stored separately. Never add cumulative
snapshots together or call transport's zero model calls zero recipient context.
The skill is 5,223 UTF-8 bytes and is loaded once; each injection contains only new
message IDs, sender, optional topic and body.

Assessment: **8/10 for this local POC**, based on working DB-first routing, durable
audit, one-time offers, generic fallback and live vendor validation. Remaining
limits are version-sensitive native endpoints, explicit recovery after uncertain
submissions, manual host integration where no API exists, no automatic audit
archive, and validation on macOS ARM64 only. This run covers one recipient per
vendor plus raw; the earlier nine-worker matrix concerns managed workers.

The final managed-worker regression also passed with real Claude Haiku, Codex
Luna and Pi Haiku using a copied standalone skill. It verified Codex→Claude→Codex
and Codex→Pi→Codex DB-backed exchanges, actual symlink/case-alias lock conflicts,
lease release, handling acknowledgements and process cleanup. See the
[managed receipt](../out/managed-final.json). Deterministic validation passed 15
Rust contracts plus 26 CLI/process tests, with Python SQLite interop enabled;
formatting, Clippy, the skill validator and archive checks passed.
