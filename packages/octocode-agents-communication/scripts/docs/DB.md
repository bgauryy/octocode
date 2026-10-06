# Local database protocol

Load for storage, unified record queries, SQL clients, or migration. The CLI, MCP, hooks and native adapters share one SQLite protocol. Initialize through `join`; conforming SQL clients must preserve its transactions and invariants.

## Identity and discovery

Default: `<Octocode home>/agents-communication/communication.sqlite`, resolved through shared config. `--database` overrides it. All collaborators share one DB. Each participant retains its actual canonical workspace.
`workspaces(workspace,coordinationScope)` maps Git workspaces to the canonical common directory.
Non-Git projects use the workspace itself as scope. Linked worktrees share coordination; independent clones remain separate. `db info` inspects without creating storage. `schema` exposes canonical SQL and fingerprints; `schema types --compact` lists every required/optional field and writer; `schema types` includes full definitions and validated examples; `schema type message` narrows one.

Application ID is 1329678147; schema version is 5. Every open checks the exact schema fingerprint. A v1/v2/v3/v4 store requires explicit migration; unrecognized schemas remain unchanged. Python 3.9+, SQLite 3.42+ **with FTS5** are required. New stores use WAL only on patched SQLite (3.51.3+, 3.50.7–3.50.x, 3.44.6–3.44.x); existing WAL requires a patched writer. The runtime keeps existing journal modes.

## Unified records

All retained events share this envelope:

```json
{"recordId":42,"path":"/absolute/workspace","from":"agent-uuid","to":"recipient-uuid","type":"message","timestamp":1791187200000,"branch":"feature/unified","data":{"messageId":17,"body":"Review the change","reasoning":"Resolve review before handoff","wake":"action","replyRequired":true,"ttlMs":3600000,"expiresAt":1791190800000}}
```

`path` is the canonical **workspace**, not a file path. `from`, `type`, `timestamp` and `data` are mandatory; `to` is always present (null for shared records). `branch` is optional. Time is integer Unix milliseconds. `id` identifies the record and orders pagination; mail handling uses **data.messageId**, leases use **data.leaseId**. File/context paths belong inside `data`. `from` is an exact DB identity, never a display name or vendor session ID.

`records` is the append-only event history. Its columns are the envelope plus internal `entityId` and retry `key`; neither internal field leaks into fetched envelopes. Triggers append operational events in the same transaction as their transition. `sessions`, `leases`, `lease_waits`, `messages`, `deliveries`, `dispatches`, `subscriptions`, `attachments`, `documents`, and peer-directory caches remain operational state/indexes: they enforce presence, uniqueness, expiry, replies and delivery tokens. They are not alternate generic APIs. Immutable `messages` owns authoritative bodies; fetch resolves that reference into `data`. FTS stores derived searchable content and indexes. No event-log replay is needed for a hot inbox check.

The single type owner is `catalog.json`. `schema type <name>` returns its data schema, event meaning, routing, validated example and suggested fetch filter, without opening a DB. `schema types --compact` gives the complete field inventory without repeating schemas/examples/SQL. `?` means optional for current v4 output. Hook-only message references are a separate validated variant.

| Type | data fields | Event |
| --- | --- | --- |
| `coordinate.in` | `name`, `vendor`, `vendorSession?`, `task`, `status`, `expiresAt` | A new agent registers in this workspace; data is its complete identity/presence snapshot. Writer: `join`. |
| `coordinate.update` | `name`, `vendor`, `vendorSession?`, `task`, `status`, `expiresAt` | Identity or branch changes, presence shortens while still live, or expired presence resumes; full snapshot. Ordinary unchanged heartbeats create no event. Writer: `heartbeat/attach`. |
| `coordinate.out` | `name`, `vendor`, `vendorSession?`, `task`, `status`, `expiresAt` | Explicit leave ends presence; same from/path and full agent snapshot as in. Clock expiry or a crash creates no synthetic out event. Writer: `leave`. |
| `coordinate.profile` | `name`, `vendor`, `vendorSession?`, `task`, `status`, `expiresAt` | Task or status changes; full identity/presence snapshot at that moment. Writer: `set_status/heartbeat`. |
| `message` | `messageId`, `body`, `ttlMs`, `expiresAt`, `reasoning`, `wake`, `replyRequired`, `key?`, `topic?`, `conversationId?`, `replyTo?`, `hook reference: bodyOmitted:true,next` | Immutable mail committed for fixed recipients. from is author; to is agent ID, topic:<topic>, or *. Complete uses data.messageId, never recordId. Writer: `send_message/notify_all/complete`. |
| `lease.acquired` | `path`, `kind`, `reasoning`, `leaseId`, `expiresAt` | A successful reservation commits. Full path/kind/deadline snapshot; from is lease owner. Writer: `lock/lock_many`. |
| `lease.renewed` | `path`, `kind`, `reasoning`, `leaseId`, `expiresAt` | A lease deadline changes. Full lease snapshot, including path/kind, without another lookup. Writer: `renew/heartbeat`. |
| `lease.removed` | `path`, `kind`, `reasoning`, `leaseId`, `expiresAt` | Unlock, leave or prune removes a lease. Full final snapshot; expiresAt is its former deadline, not removal time. Writer: `unlock/leave/prune`. |
| `subscription.added` | `topic` | An agent starts receiving a topic; subscribe replaces its topic set. Writer: `subscribe`. |
| `subscription.removed` | `topic` | An agent unsubscribes or leaves. Already-sent message recipients remain unchanged. Writer: `subscribe/leave`. |
| `attachment.created` | `transport`, `endpoint` | A host transport is registered for an agent. Endpoint may be null; registration does not prove delivery. Writer: `attach`. |
| `attachment.updated` | `transport`, `endpoint` | Host transport or its attachment metadata is refreshed; reattaching can emit this even without a transport change. Writer: `attach`. |
| `delivery.created` | `messageId` | One fixed recipient is added to a message. from is recipient; to is original sender. This is storage, not submission or handling. Writer: `send_message/notify_all/complete`. |
| `delivery.claimed` | `messageId`, `owner` | A delivery owner claims one recipient/message for external delivery. owner is a dispatcher claim token, not an agent ID. Writer: `delivery owner`. |
| `delivery.acknowledged` | `messageId` | Recipient completes handling; from is recipient, to original sender. This receipt is not proof of correctness or approval. Writer: `complete`. |
| `dispatch.staged` | `messageId`, `token`, `transport`, `error?` | A recipient/message attempt is reserved with token before external I/O. from is recipient, to original sender. Writer: `delivery owner`. |
| `dispatch.submitted` | `messageId`, `token`, `error?`, `transport` | The host transport accepted an attempt. Submission is not acknowledgement or evidence of model consumption. Writer: `delivery owner`. |
| `dispatch.uncertain` | `messageId`, `token`, `error?`, `transport` | Transport failed or its receipt is unknown; the message may have arrived. Inspect before explicit retry. Writer: `delivery owner`. |
| `dispatch.ready` | `messageId`, `token`, `error?`, `transport` | Explicit retry reopens an unacknowledged delivery. token identifies the old attempt until the next stage replaces it. Writer: `delivery owner`. |
| `document` | `name`, `path`, `author`, `reasoning`, `bytes`, `sha256`, `context?` | Immutable document publication metadata; content stays in data.path on disk. Optional context annotates indexed discovery. Writer: `share_document`. |
| `usage` | `key`, `scope`, `model?`, `inputTokens?`, `outputTokens?`, `cachedInputTokens?`, `contextTokens?`, `cacheWriteTokens?` | Original host counters, once per key. scope is request, turn or cumulative; absent counters are unknown and cumulative values must not be summed. Writer: `record_usage`. |
| `host.context` | `generation`, `vendor` | Hook records offering this binding/context generation once. This is a receipt, not the injected text or proof it was read. Writer: `host hook`. |
| `host.inbound` | `vendor`, `permissionMode`, `crossSessionInbound?` | Claude hook report of the receiver's permission mode and visible crossSessionInbound setting, appended when either changes. Dispatch predicts whether Claude Code would hold or refuse peer socket messages from the latest report. Writer: `host hook`. |
| `memory` | `content`, `tags?`, `expiresAt?`, `scopePath?`, `arbitrary extra JSON` | A retained fact: required content, optional tags/scopePath/expiry, plus arbitrary JSON. expiresAt is metadata; it does not auto-hide or delete records. Writer: `record`. |
| `event` | `name`, `arbitrary extra JSON` | A custom occurrence identified by name, plus arbitrary JSON; recording does not send notifications. Writer: `record`. |

Coordination snapshots include `name`, `vendor`, optional native `vendorSession`, `task`, `status`, and `expiresAt` in every in/update/profile/out event. In and out use the same agent ID and workspace; out means leaving this workspace, not a second outside-repo identity. Repository-shared history retains each record’s originating workspace path. Use `peers` for current presence instead of replaying events. Lease renew/remove events also retain path/kind/deadline, so an agent can interpret them without another lookup.

`document.context` contains required `summary`, optional `path`, `kind`, `branch`, `ttlMs`, and computed `expiresAt`. `usage.scope` is request/turn/cumulative; its optional counters are inputTokens/outputTokens/cachedInputTokens/contextTokens/cacheWriteTokens and optional model. Attachment endpoint and dispatch error may be JSON null. `delivery.claimed.owner` is a dispatcher claim identifier. `host.context.generation` identifies a binding receipt, not a conversation message.

Current schemas describe v5 writers. Migrated v1/v2/v3/v4 payloads remain exactly as originally recorded; missing historical fields are unknown. In particular, old out/profile events may have no vendor/name and old host.context data may be empty. Migration does not fill them from today's identity or rewrite their chronology.

`record {type:"memory",data:{content:"…"}}` or `record {type:"event",data:{name:"checkpoint",...}}` appends without messaging/waking anyone. `to` optionally limits visibility; null shares with the repository coordination scope. Input may set optional `branch` (null suppresses the session default) and sender/type-scoped retry `key`. Generic JSON remains intact, including nulls, booleans and nested fields; data is limited to 16384 UTF-8 bytes. Operational types require their dedicated validated transitions. Custom records are append-only; expiry in memory is metadata, not automatic deletion or filtering.

## Smart fetch

```sh
comm fetch '{"incoming":true}' --session AGENT_ID
comm fetch '{"type":"message","where":{"messageId":17}}' --session AGENT_ID
comm fetch '{"type":"memory","search":"unicode paths","branch":"feature/unified"}' --session AGENT_ID
comm fetch '{"types":["message","delivery.acknowledged"],"from":"AGENT_ID","since":1791187200000}' --session AGENT_ID
```

`fetch` accepts exact `id`, `type` or `types`, `from`, `to`, `branch`, inclusive `since`/`until`, text `search`, scalar `where` on dotted payload fields, and `incoming:true`. Text searches AND literal words/quoted phrases through FTS5; raw query syntax is treated literally. Missing fields differ from JSON null, booleans differ from numbers. Pair `where` with type/from/time when possible. Bound CLI/MCP paths cannot be changed through query data.

Results contain `items`, fixed `through` high-water ID, and executable `next.command`/`next.input` when more exist. Run it unchanged. Ascending IDs and an exclusive `after` cursor reach all matching rows through the ceiling, without duplicating later inserts. Each page targets 16 KiB and at most `limit` (1–100); one oversized row is intact with a budget diagnostic. An incoming query evaluates current expiry/ack state on each page; restart from zero at a later task boundary. Reads never claim, submit, acknowledge, or renew.

Mail and delivery/dispatch history is visible only to the sender and the recipients captured at send time, including topic/broadcast mail. Targeted memories/events are visible to author/recipient. Shared records and coordination metadata use repository scope; lease ownership stays in the current worktree. An expired bound identity may inspect history; writes require live presence. Inaccessible records yield no rows. Identity strings are routing data; a process with trusted-user filesystem access can read or replace SQLite directly.

`current:true` restricts dispatch history to each recipient/message's latest attempt whose token and state still match live dispatch state. Pi recovery combines it with `type:"dispatch.staged"`, the recipient's `from` ID and `where.transport`. Completed or superseded attempts never replay. CLI/MCP discovery carries type-specific data schemas; MCP/Pi derive retry keys from stable tool-call IDs when omitted.

### Lookup indexes

| Index | Purpose |
| --- | --- |
| records_type(path,type,id), records_from(path,from,id), records_to(path,to,id) | narrow event streams and ordered paging |
| records_time(path,timestamp,id), records_branch(path,branch,type,id) | time/branch scopes |
| records_entity(type,entityId,id) | resolve mail/lease events without scanning history |
| records_dispatch(path,from,entityId,id) | latest dispatch attempt during receipt recovery |
| records_search FTS5 | indexed words/phrases across payloads and message bodies |
| deliveries_inbox(recipient,acknowledgedAt,message) | fast pending inbox and hooks |
| leases_path(workspace,pathKey), leases_owner(owner) | overlap queries and ownership cleanup |
| lease_waits_order(workspace,id) | oldest-first lease grants |
| sessions_scope(workspace,expiresAt), sessions_host(workspace,vendorSession) | peers and host identity |
| subscriptions_topic(topic), documents_scan(workspace,id) | fanout and context |

## Connection and transactions

Use local storage, bound parameters, foreign keys, 5000 ms busy timeout, and FULL synchronous. Reads take snapshots; state changes use short `BEGIN IMMEDIATE` transactions before validating identity/routing/ownership. Commit or roll back the whole transition. No transaction waits for a model or filesystem edit. Database replacement stops resident workers. Schema fingerprints derive from normalized sqlite_schema rows; new files are private (0600), created directories 0700.

## Presence

Join creates a UUID and 60-second presence. It captures the current Git branch when omitted (detached/non-Git remains absent); heartbeat can explicitly update/clear branch. Hooks capture branch at registration, not on every idle check. Hosts that switch branches update heartbeat. Heartbeat maintains live presence, normally every 15s; `renewLeases:true` extends live leases without shortening longer deadlines or reviving expired ones. Resume requires an expired matching-vendor identity and discards stale leases/claims. Leave expires presence and releases leases, subscriptions, and pending claims. It does not acknowledge mail or clear uncertain dispatches. Normal unchanged heartbeats append no events. Expiry remains clock-based, even without a coordinate.out event after a crash.

## Required intent

New mail, leases and documents require nonblank `reasoning`, at most 512 UTF-8 bytes. It is immutable, describes purpose, and is not a reasoning transcript. Retry keys require unchanged payload/routing. Generic events/memories use their typed data contract.

## Path leases

Paths resolve through real filesystem ancestors and stay inside the case-preserving workspace. Comparison uses pinned Unicode 16 NFD(casefold(NFD(component))) keys; exact, ancestor-tree and descendant-range lookups are indexed. Only `ok:true` from lock/atomic lock_many grants ownership; a bundle conflict acquires none. Leases need both live owner and live expiry. TTL is 1000–600000 ms; renew/unlock check exact ID, owner, presence and expiry. False renewal requires reacquisition. `locks` reports live state; `presence:expired/all` reports retained operational rows, while fetch includes removed lease events. `check_paths`/`check_write` are read-only checks, never acquisition. Reservations and optional guards are advisory and do not fence OS/shell writes. Coordinate contested paths and keep useful independent work moving; release leases no longer needed.

`lock`/`lock_many` with `wait:true` queue a conflicting request in `lease_waits` (one per identity, 10 minutes, holding nothing) and send each blocking owner a passive notice. Unlock, leave, resume, prune, heartbeat and every lock first grant waits, oldest first, whose targets are all free, in the same transaction; the grant inserts the leases and sends the waiter an action-wake "Lease granted" message from itself. A silently expired lease therefore passes to the next waiter at the next heartbeat or lock by any identity. A wait is refused when a blocking owner already waits, directly or transitively, for a lease the requester holds. A set wait never holds part of its set; a later single-path request may take a path while the set waits. Leave and resume drop the identity's wait; a direct lock of an overlapping path ends it.

## Shared handoff documents

Publish at most 1 MiB UTF-8 with `share_document`.
New Git documents use `<git-common-dir>/octocode-communication/`; non-Git documents use `<workspace>/.octocode/communication/`.
Repository-authorized reads retain existing document paths and verify their hashes.
If historical names collide, set `read_document.workspace` to the author’s actual worktree.
Read continuations preserve that origin selector. Context items retain their originating workspace and branch. Names are immutable lowercase filenames without traversal; identical retries retain metadata, revisions use new names. Content lives on disk once; the document record owns hash/bytes/path/author/context. Publication uses a synced temp file and no-clobber link followed by DB registration. A crash may leave an unregistered file; preserve it. `read_document` verifies all bytes/hash and returns aligned UTF-8 byte pages. `context` indexes live path/branch notes with explicit scanned coverage and continuations. Preserve files separately from DB snapshots.

## Messages and notifications

Send validates live author and exactly one direct ID or topic, then commits immutable mail and fixed delivery recipients atomically. Direct targets can be offline but must share `coordinationScope`. Topics and notify_all snapshot active peers (sender excluded); later joins get no backfill. Direct defaults require an answer/action wake; fanout defaults no reply/passive. Correlation is optional ASCII conversationId; final replies inherit it. Inbox is the convenience pending-mail projection; fetch incoming returns the unified shape. Message TTL is 1000–86400000 ms and retries never refresh it.

### Atomic final reply

`complete {message:ID,reply:"answer"}` commits the final direct answer and parent acknowledgement atomically, with reserved idempotency key complete:ID. Use **data.messageId** from a fetched/hook envelope. Changed retries fail and all errors roll back. Progress uses a new FYI, not replyTo. Silent complete handles 1–100 unique received IDs atomically.

### Reply requirements

Required requests need a final answer; informational messages forbid replies. Keep unfinished work pending. Wake intent is independent of reply policy and user authority. A receipt proves storage/submission/handling, not correctness or approval.

## One-time delivery and host hooks

Attach binds an existing host session; one owner stages a bounded batch/token in SQLite before external I/O. Success records submitted; failure records uncertain. Neither means acknowledgement. Hooks/native adapters deliver the same record envelope; ordinary empty checks use indexed state and data-version fast paths. Staged/submitted/uncertain attempts never auto-replay; inspect recipient/health before explicit retry_delivery. Raw recovery can always fetch pending mail. Host budget overflow preserves each envelope with `data.messageId` and `data.bodyOmitted:true`; `data.next.command`/`data.next.input` reaches the intact payload unchanged with the current binding. Pi confirms persisted host-ledger receipts; generic hooks record output submission only. Setup, trust and supported events: [HOST_SETUP.md](HOST_SETUP.md), [HOST_HOOKS.md](HOST_HOOKS.md). Native wake/receipts: [SERVICE_PROTOCOL.md](SERVICE_PROTOCOL.md).

## Records and usage

Record triggers preserve operational changes atomically, including conforming SQL clients; append-only guards reject update/delete. Usage retains request/turn/cumulative scope and original provider semantics; never sum cumulative or overlapping counters. Native attachment cannot infer host inference usage. The host owns record_usage. Missing counters are unknown.

## Cleanup and conformance

Prune removes up to 100 expired leases per workspace transaction with continuation; expiry does not authorize deleting records/mail. Compact reclaims reusable pages without deleting evidence. Export publishes a verified no-clobber full DB snapshot (all workspaces); document files need separate preservation. Stop workers before restore or migration; never replace a live DB or mix unrelated WAL files. Owner lock files contain no data and are excluded from snapshots.

V1/V2/V3 migration, with all old clients stopped:

```sh
comm db migrate '{"backup":"/absolute/new-backup.sqlite"}'
```

Only the pinned v1/v2/v3/v4 fingerprints are accepted. The command holds a writer lock, makes and verifies a private no-clobber backup, and verifies fingerprint/integrity/foreign keys in one transaction. V1 converts every event to an envelope preserving IDs/state/keys/document references and rebuilds search indexes; v2/v3/v4 replace triggers and add missing tables (v4 gains `lease_waits`) without rewriting historical records.
V4 adds workspace coordination mappings without changing retained record IDs or origin paths. Failure rolls back; a published backup may remain. V1 had no branch metadata, so those migrated records omit branch. Restart clients on v4; no legacy API aliases or mixed-schema clients are supported. Keep the backup and document files. Missing historical identity fields stay unknown; no migration invents old snapshots.
