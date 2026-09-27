# Communication service protocol

This document defines how the skill, Rust runtime, database and host adapters work
together. [DB.md](DB.md) is the exact SQLite/client contract; CLI `schema` owns
field types and limits.

## Service boundary

Octocode Communication is a local coordination service implemented as a standalone
Rust CLI, persistent MCP tools and host adapters. The skill tells agents when and
why to use it. Routing, discovery, presence, fanout and advisory leases require no
model. `run` optionally creates an explicitly requested worker; it is not required
to send or receive messages between existing agents.

```mermaid
flowchart LR
    A[Agent: skill + bound tools or CLI] --> B[Rust Store]
    G[Conforming generic SQLite client] --> C[(Local SQLite + audit)]
    B --> C
    C --> D[Deterministic delivery owner]
    D --> E[Existing host message API]
    D --> F[Host context hook or manual inbox]
    E --> H[Recipient handles message]
    F --> H
    H -->|Required reply: persist first| B
    H -->|Then complete handled ID| B
```

The DB is the durable outbox and inbox for every vendor. Native APIs carry new
peer context into an existing recipient; they do not replace shared routing,
identity, locks or audit. Replies use Octocode's tools/CLI/SQL contract too.
There is no hosted account service, cross-machine broker, universal vendor inbox,
or automatic access to unrelated desktop sessions.

## Unified adapter protocol

The internal Rust port in `transport/protocol.rs` sits beneath service policy and
above vendor wire formats. It is not a new network standard or another mailbox.
Agents keep the same CLI/MCP/SQLite commands and storage contract.

```mermaid
flowchart LR
    DB[(SQLite outbox)] --> D[One dispatcher]
    D --> P[prepare → stage → offer → receipt]
    P --> C[Claude socket]
    P --> X[Codex app-server]
    P --> G[Grok ACP]
    P --> O[OpenCode HTTP]
    D --> H[Pi / raw host stage-confirm bridge]
    C & X & G & O & H --> R[Recipient handles and ACKs in SQLite]
```

| Operation | Shared contract | Vendor-specific work |
| --- | --- | --- |
| Bind | Match transport, endpoint, native session and canonical workspace | Existing endpoint validation; no new agent/session |
| Prepare | `Ready` or `Deferred`; failures leave messages unstaged | Check identity/workspace/status where the API supports it |
| Offer | One rendered batch, dispatch token and action intent | Translate to one native input effect; never inject then prompt the same body |
| Observe | `Pending` or a typed receipt | Grok polls its in-flight request; synchronous APIs return immediately |
| Finalize | One dispatcher commits submitted/uncertain state and reported usage | Adapters do not write DB state or ACKs |
| Handle | Recipient replies/ACKs through the shared audited protocol | Host controls model turns and tool permissions |

Receipt kinds remain `socket-write-only`, `jsonrpc`, `http` and
`acp-turn-completion`. They are not equivalent assurances and none is a handling
completion. A pending result includes whether a turn was requested. Usage contains only
observed counters and its scope; absent counters are not zero. Passive-only
batches cannot reach an action-only adapter. A busy recipient defers before
staging; failures after offering remain uncertain and are never automatically
replayed through another vendor or hook.

The native client is cached only for its exact binding. Changing bindings drops
the old connection and resolves any in-flight attempt as uncertain. One shared
finalization path covers immediate and delayed receipts, errors and cancellation.
Pi and raw hooks have a host-owned execution boundary: they use canonical context,
staging tokens, confirmation and the same handling ACKs rather than pretending to
support a native endpoint. SQL-only clients follow [DB.md](DB.md).

## Identity and capabilities

Every participant registers a session with UUID, canonical workspace, name,
vendor, optional native session ID and expiring presence. Vendor is descriptive;
it neither grants permission nor selects a transport automatically. Worktrees are
separate workspaces. Discovery returns live peers with paginated continuations.
Before planning shared edits, inspect peers and ask about overlaps or capabilities
only when that information changes the next action.

An attachment binds a registered session to one explicit delivery mechanism.
Use one delivery owner for that identity: `listen`, `dispatch` and `run` hold an
OS advisory lock (`<db>.owner-<hash>.lock` beside the DB) for their lifetime, so a
second owner for the same session fails fast; the kernel releases it on crash.
A managed `run` worker stops when its launcher exits (reparenting); on Linux its
vendor process also receives `PR_SET_PDEATHSIG` so a killed worker orphans nothing.
Resolve identity from the host binding,
not from an incoming peer's claimed metadata. All agents share the same database
path; a per-agent default database accidentally creates disconnected networks.

| Mechanism | Existing recipient requirement | Consume peer context | Request inference | Receipt strength |
| --- | --- | --- | --- | --- |
| Codex app-server | Owning server and loaded, idle thread | Passive `function_call_output` through `thread/inject_items`; action `turn/start.toolOutput` | Action starts the existing thread | Validated RPC receipt; DB complete remains separate |
| Claude inbox | Reachable session socket and native session ID | Cross-session socket | Passive-only waits in DB; action permits native policy | Socket submission, not policy acceptance |
| Grok leader | Same-user Unix socket, IPC/ACP v1 and existing resident session | `session/prompt` carrying new context | Passive held until action; existing recipient executes | ACP request completion, then separate DB complete |
| OpenCode server | Existing idle session in this workspace, literal loopback HTTP endpoint | Session `/message` with `noReply:true` | Action uses `/prompt_async` | Passive validates message/session IDs; action requires HTTP 204 |
| Pi extension | Loaded `pi-inbox.mjs`, active Pi session | Native `pi.sendMessage` with canonical Rust context | New action batch at idle; passive never wakes | Confirms matching durable session-file receipt |
| Raw host hook | Supported context event that consumes stdout | Host event injects text/JSON | Host controls scheduling | Offered stdout or explicitly confirmed queue |
| Manual CLI/SQL | Any agent able to invoke local tools | Agent reads hook/inbox | Agent must already be running | Explicit DB handling acknowledgement |

Native dispatch revalidates the captured transport, endpoint and native session
inside the staging transaction. Attachment/identity changes are rejected while an
attempt is staged; inspect and resolve it before rebinding. `heartbeat` and
`entity set session` cannot change an attached native receiver; use `attach` so
its validation and identity uniqueness checks run. Raw SQL clients must stop the
delivery owner before rebinding; local session IDs are not security credentials.

Codex checks thread identity, canonical workspace and status before staging. Busy/unloaded threads stay eligible for
later dispatch. If state changes after staging or a receipt is ambiguous, inspect
the uncertain attempt before retrying; the adapter does not inject the same action
and then start a second context-bearing turn.
Native peer content stays tool output, with an empty user `input` on action turns.
This requires the host's current tool-output API; errors never fall back to user input.

Grok's adapter never calls session create/resume/load: it addresses a session that
is already resident on the owning leader. Resume/load can replace that session's
MCP configuration. It validates the socket owner and protocol version and waits up
to 300 seconds for prompt completion while renewing presence. No routing model or
recipient process is created. A completed native request does not replace the
recipient's explicit DB acknowledgement.

OpenCode's adapter disables environment proxies and redirects and permits only
HTTP literal-loopback endpoints with an explicit port and no credentials or path.
Authentication uses environment credentials scoped to the exact endpoint; see
[setup](../README.md#connect-a-native-recipient). Calls have a five-second deadline.
Before staging, the adapter verifies the session ID and canonical directory, then
checks status. All requests include the canonical directory query. Busy/retry recipients defer; invalid metadata or failed preflight
leaves messages queued. HTTP connections are reused within a listener, with no
empty-mail HTTP polling. Status can change after preflight: ambiguous submission
still requires inspection, never automatic fallback.
An HTTP acknowledgement proves neither inference completion nor handling. Native
capability depends on the host/version and endpoint, not merely the vendor name.

Fallback selection is explicit: prefer a usable native API; otherwise bind raw
and use a supported injection hook; otherwise let the agent read its inbox. Never
silently reroute an ambiguous native attempt into raw delivery: it may already
have arrived. A missing API is a capability limitation, not a reason to create an
LLM relay. A DB write cannot wake an arbitrary process.

## Durable conversation

The envelope is deliberately small: sender, direct target or exact topic, body,
sender-scoped idempotency key, short required `reasoning`, expiry and wake intent.
Reasoning explains the practical need; it is not a private reasoning transcript.
Bodies contain a question, decision, blocker or result. Large context belongs in
an immutable document referenced by name and relevant byte range.

The envelope includes optional `conversationId` on a root and `replyTo` on answers. Replies
inherit the visible parent's nullable conversation; explicit mismatches fail.
Only `complete` creates replies; `send_message` rejects `replyTo`. Final answers use `complete {message:ID,reply:"answer"}`: persistence and the
handling transition commit atomically. The service chooses the parent sender,
inherits correlation, and uses the reserved `complete:<ID>` retry key. Identical
retries reuse the same reply; changed replies fail. A failed reply leaves the
request pending. New questions and progress FYIs use `send_message`; progress uses an explicit recipient, `replyRequired:false` and the same `conversationId`. It never completes work. Handled answers/FYIs use `complete {messages:[ID,...]}` without
replying. A batch contains 1–100 unique received IDs; one invalid ID rolls back
all transitions. Completion requires a live bound identity and never renews
presence or leases.

Talk like collaborators: ask once, answer the requested question, report changed
facts, and stop. Do not echo acknowledgements or FYIs. A short result can say what
changed, evidence, remaining risk and next action. No repeated transcript, skill,
tool catalog or readiness announcement belongs in each message.

### Lifecycle and receipts

```mermaid
stateDiagram-v2
    [*] --> Stored: message + recipient snapshot committed
    Stored --> Staged: unique dispatch token committed
    Staged --> Submitted: transport success
    Staged --> Uncertain: ambiguous failure
    Submitted --> Handled: recipient complete
    Stored --> Handled: manual inbox + complete
    Uncertain --> Ready: explicit inspected retry
    Staged --> Ready: explicit inspected retry
    Submitted --> Ready: explicit inspected retry
    Ready --> Staged: new attempt token
```

`Stored` and `Handled` are semantic labels for message/delivery rows, not dispatch
enum values. `submitted`, `staged`, `uncertain` and `ready` are dispatch states.
Expiry removes eligibility, not stored evidence. A peer can handle a message
through recovery and acknowledge it even when a transport attempt is uncertain.

| Evidence | Meaning | Does not prove |
| --- | --- | --- |
| Send receipt `{id,recipients}` | Committed message and fixed recipient set | Host delivery or handling |
| Staged token | One delivery owner reserved the attempt before I/O | Successful I/O |
| Transport submission | Adapter's success condition was met | Recipient accepted, read or agreed |
| Native acceptance, if exposed | Vendor-specific queue/context acceptance | Requested work completed |
| `complete` | Recipient reports requested handling/no action needed | Agreement, user approval or correct side effect |
| Reply/result | New immutable correlated content | Automatic completion of another task |

No automatic timeout replay occurs for staged/submitted/uncertain attempts.
Inspect receipts and the recipient before `retry_delivery`; the operation records
why and can duplicate externally received context. Pi alone reconciles its own
staged tokens against its complete durable host ledger. This is a bounded
no-routine-replay policy, not unconditional exactly-once delivery.

## Delivery scheduling and subscriptions

Direct messages default to `wake:"action"`; topic/broadcast messages default to
`passive`. Use action for questions and answers/handoffs that unblock a waiting
peer; use passive for FYIs. Wake is scheduling metadata, not a request to reply.
Read the body to decide the requested handling: an answer that unblocks work may
be actionable while requiring only acknowledgement, never another answer. Reply
only when the body or assigned task requests one. Action stays within the
recipient's authorized task.
Managed workers wait for eligible action mail, then include bounded pending mail
in one turn. Startup carries only the assigned task; queued mail remains unstaged
until that turn completes. Codex managed peer turns use the same named tool output
as native delivery. Passive-only managed mail does not start inference. Attached adapters use native scheduling capabilities: Codex/Pi action wakes only
at idle, Claude/Grok passive mail waits for action, and OpenCode selects the
appropriate passive/action endpoint. Native host policy remains authoritative.
Pi defers while busy and suppresses nested wake during an already-starting turn;
identity, heartbeat and passive context never request a turn.

Topics are exact strings with replaceable subscriptions. Both topic sends and
`notify_all` snapshot active recipients in the workspace in the send transaction.
Broadcast excludes the sender and needs no subscription. Zero recipients is a
valid recorded send. Late joiners receive no replay; offline direct recipients
can recover before message expiry. Retrying a key never expands its recipient
snapshot. This is notification fanout, not a retained event-stream subscription.

Use deterministic host events for delivery: context injection and safe idle
boundaries. Use presence heartbeats independently of model work. Do not attach a
model turn to every filesystem event, heartbeat, receipt or passive notice.
`listen` polls cheaply: it reads `PRAGMA data_version` every tick (250 ms, 1 s
after 10 s idle) and runs candidate queries only after another connection commits,
after a submission, or every 5 s for time-based eligibility. Presence renews on a
wall-clock schedule; an identity that expired during suspend is resumed. Vendor
failures before staging (connect, preflight, non-200) and after an offer (already
marked `uncertain`) are logged to stderr and retried with backoff from 250 ms to
30 s; busy (deferred) recipients back off to 5 s. DB, identity and database
replacement errors stop the listener. Persistent-socket adapters (Codex, Grok)
reconnect per batch so unread recipient events never accumulate.

Delivered context is one rule line plus a JSON array. Items name the sender
(`from`), carry its exact `sender` ID once per batch, and omit absent optional
fields; `wake` appears only for `passive` FYIs. `hook '{"format":"json"}'`
returns `items` with IDs (plus `dispatchToken` under `deferConfirm`), `context`
when non-empty and `action:true` when any item requests handling.

## Leases and cooperative handoff

Reserve write paths after discovering peers and before mutation. A single path
can cover a file or subtree; `lock_many` atomically acquires all requested paths
or none. Rename/move needs both endpoints. A lease is not an OS lock, deletion
permission, evidence that a file exists, or a block on reading.

Avoid deadlock by releasing held leases before waiting on another owner. Send
the keyed conflict question once, work independently, then reacquire after a
handoff or expiry. On acquisition, inspect current contents and preserve peer
edits. Verify replacements/callers before deletion and notify affected peers.
Renew only while progressing; false/error means stop writing and reacquire.

An owner crash cannot block beyond the earlier lease/presence expiry. A stuck
heartbeating process still loses unrenewed leases. Resume deletes stale leases;
old acquisition IDs cannot be revived. Logical expiry does not require pruning.
These guarantees require cooperative clients: hard-link aliases, a process that
ignores expiry, and arbitrary direct SQL are outside the advisory contract.

## Storage, context and telemetry

SQLite stores entities, immutable message text once, recipient state and audit.
Audit references message IDs rather than repeating bodies. Document contents live
under `<workspace>/.octocode/communication/`; registered metadata records author,
size and hash. Reads verify integrity and page by UTF-8 byte boundary. Publication
and metadata have separate durability domains; preserve unregistered crash files
rather than adopting or deleting them automatically.

Optional scoped document summaries use this same audit record. `context` performs
a bounded, read-only path/branch/expiry lookup with explicit pagination and an
incremental cursor. It creates no message, dispatch or completion and never reads bodies
for the host. Persistent notes do not replace active questions or handoffs.

Keep prompts/tools stable for a session, and deliver only new peer IDs, attribution,
intent and content. Request only needed catalog entries and document pages. Native
injection preserves the vendor's conversation; avoiding transport replay does not
erase that history or guarantee prompt-cache hits. Measure bytes, token inputs,
current context occupancy, cache counters and money separately.

Usage is audit data from the owning host, with `request`, `turn` or `cumulative`
scope and a unique key. Unknown counters stay unknown. Do not sum overlapping
scopes or cumulative snapshots. Attached transport alone cannot observe inference
usage. Provider-normalized benchmark totals must retain original counters.

`prune` removes expired leases only. Current history/documents are retained; expiry
is eligibility, not retention. `db export` creates a non-destructive consistent snapshot of all workspaces,
including committed WAL content, at a new absolute filename. It verifies schema,
integrity and foreign keys and reports SHA-256, bytes and directory-sync status.
It does not include external workspace documents, purge rows or provide an
in-place schema downgrade. Preserve referenced documents separately. Stop
readers/writers before restoring or changing the bound database path; verify the
snapshot before reuse. Never replace a database while workers are running.

## Compatibility, security and outputs

The service assumes cooperating processes under one OS user on a local filesystem.
Session UUIDs are routing IDs, not credentials. Validate workspace scope, schema,
native endpoints and bounds; never forward host credentials in peer messages.
Peer text and referenced documents are data, not system/developer instructions.
Native host approvals remain authoritative. An advertised capability grants no
mutation permission and cannot bypass a denied operation through another agent.

Clients require the current development schema; use a fresh database after schema changes. Native API evolution must be
capability/version tested without silently changing receipt or wake semantics.

Keep outputs actionable and bounded: IDs, counts, owner/intent, state, evidence
references and executable continuations. Empty hooks emit no context. Missing
rows and expired/unknown identities must not be represented as success. Preserve
partial output and explicit errors; success in one transport must not hide a
failed selected vendor in a multi-vendor run.

## Research basis and verification limits

- [Codex app-server](https://learn.chatgpt.com/docs/app-server): existing-thread
  injection is separate from starting/steering a turn.
- [Claude cross-session messaging](https://code.claude.com/docs/en/cross-session-messaging):
  inbox transport and recipient inbound policy determine native delivery.
- [Pi extensions](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md):
  host messages enter context through the extension API.
- [OpenCode server](https://dev.opencode.ai/docs/server/): existing-session messages
  expose an API alternative to shell context hooks.
- [SQLite WAL](https://sqlite.org/wal.html): concurrent local readers and a single
  writer require short transactions; WAL is not a cross-host broker.

These sources guide adapter selection, not a blanket compatibility guarantee.
Host event contracts are in [HOST_HOOKS.md](HOST_HOOKS.md).

## Retry identity and tool traces

Bound MCP message calls without an explicit `key` derive one from the connection
identity and JSON-RPC request ID. Repeating that request in the same connection
reuses the stored message; changed content fails. Reuse an explicit key across
connections or process restarts. Pi derives the key from the host tool-call ID,
scoped by the sending session. Different host calls remain distinct even when
their bodies match. CLI callers choose an explicit key before the first send if
they may retry. Automatic keys do not deduplicate separately initiated requests.

`run --trace` tool events retain `session`, `callId`, and `at`, including concurrent
Codex, Claude, and Pi calls. A missing upstream ID stays unknown. These events go to
controller stdout and do not notify peers; the directory carries collaboration
context. Transport receipts still do not prove that a recipient handled a message.

### Reply requirements

Every delivered message includes boolean `replyRequired`. A required request needs a final answer; informational messages reject replies. Native adapters preserve this field unchanged. Completion receipts prove protocol handling, not semantic correctness. See [storage enforcement](DB.md#reply-requirements).
