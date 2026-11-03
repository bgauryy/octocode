# Local database protocol

Load for storage, unified record queries, or SQL clients. The CLI, MCP, hooks and native adapters share one SQLite protocol. Initialize through `join`; conforming SQL clients must preserve its transactions and invariants.

## Identity and discovery

Default: `<OCTOCODE_HOME or ~/.octocode>/agents-communication/communication.sqlite`. `--database` overrides it. All collaborators share one DB. Each participant retains its actual canonical workspace.
`workspaces(workspace,coordinationScope)` maps Git workspaces to the canonical common directory.
Non-Git projects use the workspace itself as scope. Linked worktrees share coordination; independent clones remain separate. `db info` inspects without creating storage; `schema` exposes canonical SQL and fingerprints.

Application ID is 1329678147; schema version is 8. Every open checks the exact schema fingerprint; older and otherwise incompatible databases are refused unchanged. There is no migration or overwrite path. To move from v7, retain the old database, use its compatible binary to finish work or export it, and point v8 at a fresh explicit `--database` path. The runtime uses Node's built-in `node:sqlite` (Node.js 24.15+ links SQLite 3.51.3 **with FTS5**); no other SQLite is read. New stores use WAL only on patched SQLite (3.51.3+, 3.50.7–3.50.x, 3.44.6–3.44.x); an existing WAL store needs a Node.js release linked to a patched SQLite to write. The runtime keeps existing journal modes. SQLite opens a database path of at most 504 bytes (measured on Node 24.15.0 with SQLite 3.51.3 and Node 26.4.0 with SQLite 3.53.2: 504 opens, 505 fails with `unable to open database file`). The runtime measures the path after resolving symlinks, before it creates any directory or file, and refuses a longer one with `Database path is N bytes after resolving symlinks; SQLite opens at most 504 bytes. Use a shorter --database or --workspace path.`

## Active database registry

A process that opens a database other than the default for writing (`--database <file>`) lists it where tools that read only the default file can find it; the Pi extension reads this to honor those leases. Writer: `src/runtime/registry.ts`; reader: `src/team/peer-registry.ts` in the Pi extension. This section owns the format.

One file per process, `<OCTOCODE_HOME or ~/.octocode>/agents-communication/active-databases/<pid>.json` (folder 0700, file 0600, replaced atomically; no shared lock):

```json
{"version":1,"database":"/absolute/communication.sqlite","pid":4242,"started":"1791187200","updatedAt":1791187200000}
```

`database` is the absolute `--database` path as given. `started` is the process start time as whole Unix seconds in a decimal string (from `ps -o lstart=` read as UTC), or null where unknown (Windows). `updatedAt` is the last write in Unix milliseconds. A reader ignores a file with another `version`, a relative `database`, or a missing or mistyped field.

The writer creates the entry when a store opens for writing, rewrites it at most once a minute while the process keeps running commands, and never lists the default database or a read-only open. There is no removal on exit: a one-shot CLI call exits at once, but a lease it took lives up to 10 minutes. An entry is **live** while `updatedAt` is within 11 minutes (the 10-minute lease cap plus the 1-minute refresh), or its process still runs with the recorded start time (a pid alone when no start time was recorded). The writer deletes unreadable and non-live entries of other processes, and `.entry-*.tmp` files older than that window (left by a writer that died mid-write), whenever it writes. A reader opens each live database read-only, checks `application_id` and `user_version` as it does for the default one, and treats a missing, unreadable or unknown database as holding no leases. The registry is advisory: a failure to write it never fails a command.

## Unified records

All retained events share this envelope:

```json
{"recordId":42,"path":"/absolute/workspace","from":"agent-uuid","to":"recipient-uuid","type":"message","timestamp":1791187200000,"branch":"feature/unified","data":{"messageId":17,"parts":[{"text":"Review the change?"}],"reasoning":"Resolve review before handoff","wake":"action","replyRequired":true,"contextId":"feature-review","ttlMs":3600000,"expiresAt":1791190800000}}
```

`path` is the canonical **workspace**, not a file path. `from`, `type`, `timestamp` and `data` are mandatory; `to` is always present (null for shared records). `branch` is optional. Time is integer Unix milliseconds. `recordId` (column `id`) identifies the record and orders pagination; mail handling uses **data.messageId**, leases use **data.leaseId**. File/context paths belong inside `data`. `from` is an exact DB identity, never a display name or vendor session ID.

`records` is the append-only audit history. Its columns are the envelope plus internal `entityId` and retry `key`; neither internal field leaks into fetched envelopes. Triggers append operational events in the same transaction as their transition. `sessions`, `leases`, `lease_waits`, `messages`, `deliveries`, `dispatches`, `subscriptions`, `attachments`, `documents`, and peer-directory caches remain operational state/indexes: they enforce presence, uniqueness, expiry, replies and delivery tokens. They are not alternate generic APIs. Immutable `messages` owns authoritative parts; fetch resolves that reference into `data`. FTS stores derived searchable content and indexes. No event-log replay is needed for a hot inbox check.

`assets/catalog.json` is the single owner of the 26 record types. `schema types --compact` lists every type's required and optional fields and its writer; `schema type <name>` returns its data schema, event meaning, routing, validated example and suggested fetch filter, without opening a database. Coordination events (`coordinate.in`, `.update`, `.profile`, `.out`) carry a full identity and presence snapshot (matching `coordinate.in` and `coordinate.out` records describe the same worktree agent with the same name and vendor), and lease events a full path, kind and deadline snapshot, so no second lookup is needed. Clock expiry or a crash creates no `coordinate.out`; use `peers` for current presence.

`record {type:"memory",data:{content:"…"}}`, `record {type:"event",data:{name:"…",...}}` or `record {type:"checkpoint",data:{goal,next,...}}` appends without messaging/waking anyone; `binding` and `resume` return the latest checkpoint (Checkpoint–resume). `to` optionally limits visibility; null shares with the repository coordination scope. Input may set optional `branch` (null suppresses the session default) and sender/type-scoped retry `key`. Generic JSON remains intact, including nulls, booleans and nested fields; data is limited to 16384 UTF-8 bytes. Operational types require their dedicated validated transitions. Custom records are append-only; expiry in memory is metadata, not automatic deletion or filtering.

## Smart fetch

```sh
comm fetch '{"incoming":true}' --session AGENT_ID
comm fetch '{"type":"message","where":{"messageId":17}}' --session AGENT_ID
comm fetch '{"type":"memory","search":"unicode paths","branch":"feature/unified"}' --session AGENT_ID
comm fetch '{"types":["message","delivery.acknowledged"],"from":"AGENT_ID","since":1791187200000}' --session AGENT_ID
```

`fetch` accepts exact `recordId`, `type` or `types`, `from`, `to`, `branch`, inclusive `since`/`until`, text `search`, scalar `where` on dotted payload fields, and `incoming:true`. Text searches AND literal words/quoted phrases through FTS5; raw query syntax is treated literally. Missing fields differ from JSON null, booleans differ from numbers. Pair `where` with type/from/time when possible. Bound CLI/MCP paths cannot be changed through query data.

Results contain `items`, fixed `through` high-water ID, and executable `next.command`/`next.input` when more exist. Run it unchanged. Ascending IDs and an exclusive `after` cursor reach all matching rows through the ceiling, without duplicating later inserts. Each page targets 16 KiB and at most `limit` (1–100); one oversized row is intact with a budget diagnostic. An incoming query evaluates current expiry/ack state on each page; restart from zero at a later task boundary. Reads never claim, submit, acknowledge, or renew.

Mail and delivery/dispatch history is visible only to the sender and the recipients captured at send time, including topic/broadcast mail. Targeted memories/events are visible to author/recipient. Shared records and coordination metadata use repository scope; lease ownership stays in the current worktree. An expired bound identity may inspect history; writes require live presence. Inaccessible records yield no rows. Identity strings are routing data; a process with trusted-user filesystem access can read or replace SQLite directly.

`current:true` restricts dispatch history to each recipient/message's latest attempt whose token and state still match live dispatch state. Pi recovery combines it with `type:"dispatch.staged"`, the recipient's `from` ID and `where.transport`. Completed or superseded attempts never replay. CLI/MCP discovery carries type-specific data schemas; MCP/Pi derive idempotency keys from stable tool-call IDs when omitted.

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

Join creates a UUID and 60-second presence. It captures the current Git branch when omitted (detached/non-Git remains absent); heartbeat can explicitly update/clear branch. Hooks capture branch at registration, not on every idle check. Hosts that switch branches update heartbeat. Heartbeat maintains live presence, normally every 15s; `renewLeases:true` extends live leases without shortening longer deadlines or reviving expired ones. Resume requires an expired matching-vendor identity and discards stale leases/claims. Leave expires presence and releases leases, subscriptions, and pending claims. It does not acknowledge mail or clear uncertain dispatches. Every leave appends `coordinate.out`, also for an already-expired session or a repeated leave; it lowers `expiresAt` to min(now, expiresAt-1), so the owner-lock reclaim of the leaving process never renews that session. Normal unchanged heartbeats append no events. Expiry remains clock-based, even without a coordinate.out event after a crash.

## Required intent

New mail, leases and documents require nonblank `reasoning`, at most 512 UTF-8 bytes. Immutable purpose, not a reasoning transcript. Idempotency keys require unchanged payload/routing. Generic events/memories use their typed data contract.

## Path leases

Paths resolve through real filesystem ancestors and stay inside the case-preserving workspace. Comparison uses pinned Unicode 16 NFD(casefold(NFD(component))) keys; exact, ancestor-tree and descendant-range lookups are indexed. Only `ok:true` from lock/atomic lock_many grants ownership; a bundle conflict acquires none. Leases need both live owner and live expiry. TTL is 1000–600000 ms; renew/unlock check exact ID, owner, presence and expiry. False renewal requires reacquisition. `locks` reports live state; `presence:expired/all` reports retained operational rows, while fetch includes removed lease events. `check_paths`/`check_write` are read-only checks, never acquisition. Reservations and optional guards are advisory and do not fence OS/shell writes. Coordinate contested paths and keep useful independent work moving; release leases no longer needed.

`lock`/`lock_many` with `wait:true` queue a conflicting request in `lease_waits` (one per identity, 10 minutes, holding nothing) and send each blocking owner an informational lease notice. Unlock, leave, resume, prune, heartbeat and every lock first grant waits, oldest first, whose targets are all free, in the same transaction; the grant inserts the leases and sends the waiter an action-wake lease notice from itself. A silently expired lease therefore passes to the next waiter at the next heartbeat or lock by any identity. A live wait reserves its paths: a newcomer's `lock`, or a younger wait, on an overlapping path conflicts with it and queues behind it, so a third agent cannot take a path as it frees. A wait is refused when a blocking owner already waits, directly or transitively, for a lease the requester holds. A set wait holds none of its set until every path is free. Leave and resume drop the identity's wait; the waiter's own direct lock of an overlapping path ends it.

## Shared handoff documents

Publish at most 1 MiB UTF-8 with `share_document`.
New Git documents use `<git-common-dir>/octocode-communication/`; non-Git documents use `<workspace>/.octocode/communication/`.
Repository-authorized reads verify the stored path and hash; a name is unique per repository scope. Context items retain their originating workspace and branch. Names are immutable lowercase filenames without traversal; identical retries retain metadata, revisions use new names, and `supersedes` links an ADR revision to the published document it replaces. Content lives on disk once, byte for byte (a leading UTF-8 byte order mark is content, so an identical retry matches and pages reassemble exactly); the document record owns hash/bytes/path/author/context. Publication uses a synced temp file and no-clobber link followed by DB registration. A crash may leave an unregistered file; preserve it. `read_document` verifies all bytes/hash and returns aligned UTF-8 byte pages. `context` indexes live path/branch notes with explicit scanned coverage and continuations. Preserve files separately from DB snapshots.

## Messages and notifications

Send validates a live author and exactly one direct ID, `"*"`, or topic, then commits immutable mail and a fixed recipient snapshot atomically. Direct targets can be offline but must share `coordinationScope`. `"*"` and topic sends snapshot active peers (sender excluded); later joins get no backfill. Each message contains `parts`: text, JSON data, or a URL with optional media type and filename. `replyRequired` is explicit and defaults to false; `contextId` groups related messages; `wake` controls scheduling independently. `replyTo` links the final response created by `complete`. Inbox is the pending-delivery view; `fetch` is the unified append-only history view. Message TTL is 1000–86400000 ms and retries never refresh it.

### Atomic final reply

`complete {message:ID,outcome,reply:[parts...]}` commits a correlated final response and acknowledges the parent delivery atomically. Outcomes are `completed`, `failed`, `rejected`, and `canceled`; the response parts are stored as a normal immutable message. Silent complete uses `{message:ID}` or `{messages:[IDs...]}` for handled deliveries that do not need a reply. Use the message `id` from inbox or `data.messageId` from history/hooks, never `recordId`. Changed retries fail and all errors roll back. Progress is a new message with the same `contextId`; it does not complete the request. Payload data does not trigger check-back rules, recipient copies, lease transfer, evidence rewriting, or expiry defaults; use explicit message, artifact, and lease operations for those effects.

### Reply requirements

When the sender sets `replyRequired:true`, handle the request with `complete` and an explicit outcome and reply parts. Otherwise use silent completion for handled mail. Keep unfinished work pending. Wake intent is independent of reply policy and user authority. A send receipt proves storage, a transport receipt proves only its adapter's submission condition, acknowledgement records handling, and a reported outcome records the recipient's result; none proves correctness or user approval.

## One-time delivery and host hooks

Attach binds an existing host session; one owner stages a bounded batch/token in SQLite before external I/O. Success records submitted; failure records uncertain. Neither means acknowledgement. Hooks/native adapters deliver the same record envelope; ordinary empty checks use indexed state and data-version fast paths. Staged/submitted/uncertain attempts never auto-replay; inspect recipient/health before explicit retry_delivery, which resets staged or uncertain attempts only and refuses submitted ones (the host accepted them). It is a bounded retry: after the first, each waits a 2 s doubling backoff (`retryAfterMs`); after 3 it returns `exhausted` and sends the sender one retry-exhausted notice (key `retry-exhausted:MESSAGE:RECIPIENT`). Budget deferrals do not count. Raw recovery can always fetch pending mail. Host budget overflow preserves each envelope with `data.messageId` and `data.contentOmitted:true`; `data.next.command`/`data.next.input` reaches the intact payload unchanged with the current binding. Pi confirms persisted host-ledger receipts; generic hooks record output submission only. Setup, trust and supported events: [HOST_SETUP.md](HOST_SETUP.md), [HOST_HOOKS.md](HOST_HOOKS.md). Native wake/receipts: [SERVICE_PROTOCOL.md](SERVICE_PROTOCOL.md).

## Records and usage

Record triggers preserve operational changes atomically, including conforming SQL clients; append-only guards reject update/delete. Usage retains request/turn/cumulative scope and original provider semantics; never sum cumulative or overlapping counters. Native attachment cannot infer host inference usage. The host owns record_usage. Missing counters are unknown.

## Cleanup and conformance

Prune removes up to 100 expired leases per workspace transaction with continuation; expiry does not authorize deleting records/mail. Compact reclaims reusable pages without deleting evidence. Export publishes a verified no-clobber full DB snapshot (all workspaces); document files need separate preservation. Stop workers before restore; never replace a live DB or mix unrelated WAL files. The delivery-owner lock is a Unix-domain socket named `<database>.owner-<digest of the session>.lock` beside the database (a Windows named pipe; an address over 100 bytes moves to a private `/tmp/octocode-agents-communication-<uid>/` directory). The owner holds it by listening and records `{pid, start, ino}` (process, its start time, the socket's inode) in an atomically written sidecar `<socket>.pid`; the OS drops the listener when the process dies. Liveness is that process, never a connect: `kill(pid, 0)` plus the guards that the sidecar's inode is the socket's current inode and the start time is unchanged (a socket younger than 1 s without a matching sidecar counts as live, an owner may be mid-claim), so a frozen (SIGSTOP) owner stays live and a reused pid reads dead. An unreaped zombie reads dead too: Linux `/proc` state `Z` or `X`, or `Z` in the macOS `ps` stat. `health` probes each owner once per call. A leftover socket whose owner is gone is replaced, and `health` reads `delivery_owner_live` from the same probe. It carries no data and is excluded from snapshots. Releases before the TypeScript runtime held a `flock` file instead, and the two mechanisms do not exclude each other: during an upgrade run one runtime per database, and stop every old `listen`, `dispatch` and `run` owner before starting a new one. The Windows named-pipe path is implemented but untested.
