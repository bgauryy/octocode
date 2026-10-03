# Local database protocol

This reference defines the coordination contract for cooperating local processes. Use the bundled Python CLI when available. A generic agent can use SQLite alone if it follows the same transactions and expiry rules.

- Database writes persist messages. A deterministic dispatcher or host hook delivers notifications.
- Every participating agent joins before communicating. Replies use this same store.

## Identity and discovery

- Default path: `<Octocode home>/agents-communication/communication.sqlite`, resolved by `@octocodeai/config` (the shared Python helper is copied into the bundle). `--database` overrides it.
- `db info` reports the actual path, canonical workspace, existence, application ID, generation, schema digest, SQLite version, and journal mode. Inspecting a missing file does not create a store.
- `schema` returns the command catalog, bound tools, entity catalog, and canonical DDL under `database.sql`. `schema entities` and `schema entity <name>` narrow the output.
- Application ID, schema version, digest and foreign-key relationships derive from the single bundled `schema.sql`. Readers and writers reject a different schema.
- The CLI checks all user tables, indexes, constraints, and foreign-key declarations through the normalized `sqlite_schema` definition. It never fills in missing tables.
- Only the current schema is supported. There are no legacy schemas or migration paths.
- Direct sends default to `wake:"action"`; fanout defaults to `passive`. SQL writers supply `wake` explicitly.
- Session `task` and `status`, workspace revision counters and receiver views support peer discovery. They do not own messages or delivery state.
- Ordinary heartbeats do not change the directory revision; expiry uses the clock.

### Lookup indexes

| Index | Serves |
| --- | --- |
| `leases_path(workspace,pathKey)` | lock/lock_many, `check_paths`, lease path filters |
| `leases_owner(owner)` | leave/resume cleanup, held-lease lists |
| `sessions_scope(workspace,expiresAt)` | peers, notify_all and topic recipients |
| `sessions_host(workspace,vendorSession)` partial | host identity binding |
| `subscriptions_topic(topic)` | topic fanout |
| `documents_scan(workspace,id)` + `UNIQUE(workspace,name)` | `context`, `read_document`, `share_document` |

Schema digest: SHA-256 of UTF-8 JSON for `type,name,tbl_name,sql` rows from `sqlite_schema`, excluding names beginning `sqlite_`, ordered by `type,name`. Normalize each SQL definition by collapsing whitespace and trimming. Preserve column order; use compact JSON without ASCII escaping. `schema` exposes the expected digest; `db info` exposes expected and observed digests.

## Entities and access

| CLI name | Table | Identifier | Mutable fields or transitions | `agentIdField` |
| --- | --- | --- | --- | --- |
| `session` | `sessions` | UUID | `name`, `task`, `status`, nullable `vendorSession`; join, heartbeat, resume, leave | `id` |
| `lease` | `leases` | Monotonic acquisition ID | Lock, renew, unlock | `owner` |
| `message` | `messages` | Monotonic message ID | Send once; content is immutable | `sender` |
| `delivery` | `deliveries` | `<message>:<recipient>` | Claim and acknowledge | `recipient` |
| `subscriptions` | `subscriptions` | Session UUID for the topic collection | Replace `topics` | `session` |
| `attachment` | `attachments` | Session UUID | Explicit `attach` binding | `session` |
| `dispatch` | `dispatches` | `<message>:<recipient>` | Stage, submit, uncertain, explicit retry | `recipient` |
| `audit` | `audit` | Monotonic event ID | Append only; `record_usage` adds host telemetry | `session` |

- `schema entity <name>` declares `agentIdField`. Its values are DB agent UUIDs, not vendor session IDs, claim tokens or entity IDs. No duplicate `agentId` column is stored.
- Message `target` may be a topic. Use `sender` for authorship and delivery recipients for agents.
- Documents/context expose `author`, backed by the immutable publication audit's `session`. Internal peer views belong to `session`; the workspace-wide peer revision counter has no single owner.
- `database.relationships` describes every declared SQL foreign key, including internal tables. Each entity exposes its table's subset as `relationships`.
- Composite keys keep column order: dispatch `(message,recipient)` references delivery `(message,recipient)`.
- Messages reference their sender and optional parent message. Deliveries reference messages and recipients. Leases, subscriptions, attachments, audit and peer views reference sessions. Documents reference their publication audit row.
- A message's `target` is routing data, not a foreign key; actual recipients live in deliveries.
- Entity get/list discovery includes every public entity. Set exposes only entities with supported mutations.

Access:

- Every entity command requires `--session`. Get/list accepts an expired identity for inspection, but the identity must belong to the bound workspace.
- Session, lease, subscription, attachment, and audit metadata is visible within that workspace.
- A message is visible to its sender or a recipient. A delivery is visible to its sender or that delivery's recipient.
- Unknown or inaccessible IDs return JSON `null`.
- Rows omit SQL-null columns (an absent member means null), including inbox, peers and entity views. Lease rows never expose the internal `pathKey`.
- Identity strings are routing identifiers, not secrets. This is a trusted OS-user database, not a security boundary against another process with file access.

Get, set, list:

- Get returns one row (subscriptions aggregate into `{id,session,topics}`).
- Set accepts only your session's `name`, `task`, `status`, and nullable `vendorSession`, or your subscription collection's `topics`. Vendor and session ID stay immutable. `entity set` cannot replace lease and message transitions.
- List returns at most 100 items, targeting 16 KiB of serialized JSON, and a nullable `next: {command,input}`. Run the named command with that input unchanged.
- A single oversized row stays intact with an explicit budget diagnostic, so the list makes progress.
- Integer IDs sort numerically. Delivery and dispatch lists sort by `(message, recipient)`, so `10:…` follows `9:…`.
- Message, delivery and dispatch lists start from the caller's own sent/received index entries. Audit lists walk IDs in order and stop at the page size.
- Session/lease lists default to `presence:active`; use `expired` or `all` for history. Lease activity includes owner presence.
- Lease `path` with optional `kind` lists overlaps with the same canonical file/tree rules as acquisition, without acquiring anything.
- Filters: messages support `direction`, `topic`, `conversationId` and `replyTo`; deliveries support `message` and `acknowledged`; subscriptions support exact `topic`. `schema entity <name>` gives each complete filter schema.

## Connection and transactions

- Use a local filesystem and SQLite 3.42 or later. The Python CLI uses its interpreter's SQLite.
- New stores use WAL on patched SQLite branches (3.51.3+, 3.50.7–3.50.x, or 3.44.6–3.44.x), otherwise DELETE journaling. Existing WAL stores require a patched interpreter for writes; the runtime never changes their journal mode.
- Open an initialized store in read/write-existing mode, so a path typo does not create a separate store. The CLI's `join` initializes an empty store under a writer transaction.
- New files use mode 0600; directories it creates use mode 0700. Existing file permissions are kept.
- For every writing connection, enable foreign keys, a 5000 ms busy timeout, the journal policy above, and FULL synchronous durability.
- The CLI validates the schema fingerprint under a read snapshot (no writer lock). It takes `BEGIN IMMEDIATE` only for initialization and state-changing transitions.
- Use parameter binding.
- Begin a short `BEGIN IMMEDIATE` transaction before you check identity and read any state that controls a write. Commit the complete transition, or roll it back.
- Never hold a transaction while you wait for a model, another process, or filesystem edits.
- Retry busy errors with a bounded policy. Never treat a failed write as successful.

Limits:

- Timestamps are integer Unix milliseconds from the local wall clock. An expiry is active only while `expiresAt > now`. Clock adjustments affect lease duration.
- Names, vendors, topics, retry keys, and vendor-session strings: non-whitespace text, at most 256 UTF-16 code units. Message bodies: at most 16384. Paths: at most 4096.
- TTLs are integer milliseconds: leases 1000–600000 per acquisition/renewal; messages 1000–86400000.

## Presence

- Join inserts a UUID, canonical workspace, name, vendor, optional vendor-session ID, and `expiresAt=now+60000`.
- Heartbeat validates the active session and extends it by 60000 ms atomically. It can also update the vendor-session ID. A normal heartbeat cannot resurrect an expired identity. Send heartbeats about every 15 seconds.
- With `renewLeases:true`, the same transaction also extends live owned leases to at least `now+60000`, keeping longer deadlines. It never revives expired leases. Without the option, heartbeat changes presence only; managed hosts opt in.
- Resume requires an expired or ended session with matching workspace and vendor. In one transaction it deletes the old leases, clears unacknowledged delivery claims, and extends presence. It keeps messages, subscriptions, and identity metadata.
- Leave expires presence and deletes owned leases and subscriptions. A crashed identity keeps its subscriptions for resume. Leave can clean up an already-expired session.
- A process death needs no cleanup for logical expiry.
- DB resume does not recreate vendor history. Optional managed `run --session` starts a fresh vendor conversation; attached hosts keep ownership of their conversation.

## Required intent

- Every new `send_message`, `notify_all`, `lock` and `lock_many` input requires `reasoning`: a concrete explanation of why the action is needed, not a reasoning transcript.
- Keep it nonblank and at most 512 UTF-8 bytes. A bundle shares one reason across its paths. There is no generated fallback reason.
- The message/lease stores this immutable field. Inbox, raw/native delivery, Pi and host hooks expose message intent. Lease inspection and conflicts expose the owner's intent.
- Conflict handoff suggestions carry the requester's reason and key requests by lease plus intent.
- Message retries require identical reasoning as well as body, target and topic. Lease renew/release keep the original intent in audit.
- SQLite insert triggers require reasoning even for clients without this CLI. SQL clients insert the column with the other fields; updates cannot rewrite it. Message and lease reasoning are non-null and immutable.

## Path leases

Canonicalization:

- Canonicalize the workspace and candidate path with real filesystem ancestors, including symlinks and dangling links.
- Resolve links before you consume later `..` components. Never lexically collapse a path before its links resolve.
- Bound link traversal. Propagate permission, cycle and traversal errors.
- Reject paths outside the workspace by the case-preserving resolved path. Folded keys never authorize filesystem access.
- Nonexistent components stay logical, relative to the resolved ancestor.

Comparison:

- All filesystems share one conservative namespace: compare components with Unicode 16 canonical caseless matching, `NFD(casefold(NFD(text)))`.
- Keep stored/access paths. Apply folding only to equality and tree-prefix checks.
- So `A.txt` and `a.txt` conflict even on case-sensitive volumes, and composed/decomposed Unicode names are aliases before they exist.
- `src` contains `SRC/file` but not `src-other`.
- `schema.database.leasePathComparison` reports the pinned Unicode versions. The CLI and entity conflict queries use these same rules. Do not use SQLite's ASCII-only NOCASE or lower().
- Every participant uses one current schema and comparison policy. Mixed old/new lock clients are not supported.
- Lease timestamps change the development schema. Existing databases are kept and rejected, so use a fresh shared database.
- Leases are cooperative reservations, not OS locks. They do not fence an uncooperative writer, alias hard links, or freeze directory/symlink topology.
- Use a tree lease to change directory structure. Reacquire after a rename or symlink change.

Acquisition (one writer transaction):

1. Validate active ownership.
2. Look up live leases (own expiry and owner presence both active) in this workspace whose `pathKey` conflicts. `pathKey` is `"/" + NFD(casefold(NFD(component)))` per component, root omitted.
3. Reject when keys match, a tree lease holds an ancestor key, or a tree candidate has leases in the key range `(key+"/", key+"0")`. This includes overlap with your own leases.
4. Otherwise insert the lease with its `pathKey` (a trigger rejects a missing key), acquisition/refresh timestamps and default TTL 60000 (maximum 600000). SQLite assigns the acquisition ID.

- Overlap is three indexed lookups on `(workspace,pathKey)`: exact key, ancestor tree keys, tree range. Acquisition, `check_paths` and entity path filters share them; no call loads every lease.
- `locks {}` lists active workspace locks with owner ID, reason, `acquiredAt`, `refreshedAt` and `expiresAt`. Follow every continuation. It excludes expired leases and leases of expired owners.
- `entity list lease '{"presence":"all"}'` is history, never current ownership.

Renew and unlock:

- Both take `leaseId` and atomically check the active session, acquisition ID, matching owner, and unexpired lease.
- Renew uses the requested TTL from now and returns the new `refreshedAt` and `expiresAt`. Set both atomically; expiry is within 600000 ms of refresh.
- A false result means: stop editing and acquire a fresh lease. Never reuse IDs or revive an expired lease.
- Direct SQL that skips these checks can insert conflicting leases. SQLite serializes transactions, but protocol compliance is still required.

`lock_many`:

- Accepts 1–32 `{path,kind?}` requests and one optional `ttlMs`.
- Resolves and validates all paths first, rejects overlaps within the requested set, then checks every candidate and inserts all leases in one writer transaction. A conflict inserts nothing.
- Returned leases have separate IDs for renew/unlock.
- It does not release previously held leases: release those before you wait.

Conflicts:

- Results report workspace-relative paths, owner identity once, held lease IDs, and `retryAfterMs` from the earlier of lease expiry and current owner presence expiry. This is a snapshot, not a guaranteed future grant: the owner may renew.
- `next` proposes one sender-keyed question per conflicting lease. Send it once, do independent work, then retry after a reply or expiry.
- Self-conflicts have no message suggestion: reuse the covering lease, or release and reacquire.
- There is no fairness queue or force-steal. No operation waits for another agent while it holds a transaction.
- Heartbeats extend presence only. `leave` frees leases immediately. Process death frees them logically at the earlier expiry. A stuck but heartbeating process still loses unrenewed leases.
- `prune` removes expired leases or leases of expired owners in the bound workspace only.

Regression tests: `tests/leases.test.mjs` (concurrent reversed sets, no partial grants, alias overlap, stale IDs, stalled-owner expiry, leave, expired-owner cleanup) and `tests/storage-scale.test.mjs` (lock, conflict and `check_paths` latency with 10,000 leases and 100,000 audit rows).

## Shared handoff documents

`share_document {name,content,reasoning}`:

- Publishes up to 1 MiB UTF-8 under the canonical workspace's `.octocode/communication/<name>`.
- Names: lowercase filenames with digits, dots, dashes or underscores; no directories, traversal or symlink components.
- CLI JSON `-` reads bounded stdin for content too large for command arguments.
- The file is immutable through this API. Identical retries return the original metadata. Different content, reasoning or context metadata requires a new name.
- `document.created` audit data stores name, path, author, reasoning, byte count and SHA-256. Content stays once on disk.
- An audit trigger registers each name once per workspace in the immutable `documents` table (`id` = audit ID). It serves name lookups and `context` scans.
- The CLI writes and syncs the file before it takes the writer lock. The transaction only renames and registers it. A read snapshot answers an identical retry.
- Reasoning is concrete intent (nonblank, at most 512 UTF-8 bytes), validated by the shared runtime for CLI/MCP/Pi. DB-only publishers implement the same contract; the generic audit table does not enforce document-specific fields.
- Historical records stay readable without invented intent; republish under a new name to add intent. Update publishers to supply intent and restart resident MCP/worker processes to load the rebuilt catalog. This changes the unpublished command contract, not the DB schema.

`read_document {name,offset?,limit?}`:

- Verifies the registered hash and returns UTF-8-aligned byte pages (default 8192 bytes, limit 4–16384) with a `next: {command,input}` continuation.
- Send document names and relevant offsets in short DB messages, not repeated full contents.
- Only active identities in that workspace can read registered files.
- DB-only clients can read metadata from workspace-scoped audit and verify disk content against its hash. Use the CLI to publish, or implement the same contract.

Durability:

- Publication uses an atomic no-overwrite rename, then commits metadata in SQLite. These are separate durability domains: a crash can leave an unregistered file. Keep it and publish under a new name.
- External edits/deletions fail integrity checks. No automatic document deletion or repair occurs.
- Same-user malicious directory replacement is outside this cooperative filesystem contract.

## Messages and notifications

`notify_all`:

- Snapshots every other active session in the current workspace, without subscriptions.
- In the same writer transaction as the message insert, select `id FROM sessions WHERE workspace=? AND expiresAt>? AND id<>?` and insert one delivery per selected ID.
- Encode the broadcast as `target="*"`, `topic=NULL`. Session IDs are UUIDs, so `*` is reserved.
- Late joiners and inactive/other-workspace peers get no backfill.
- A retry of the sender/key returns the original receipt and recipient count, without expanding its snapshot. Zero recipients is a successful stored broadcast.
- Inbox/complete clients consume broadcasts through the same deliveries protocol.

Send:

- Validates the sender's active session and exactly one target: direct session ID or exact topic.
- In one writer transaction: check `(sender,key)` idempotency, insert the immutable message, insert its delivery rows.
- A repeated key returns the existing receipt only when target, topic, body, reasoning, wake, resolved correlation and `ttlMs` match. Changed content fails. A retry does not refresh expiry.
- Default message TTL is 3600000 ms.
- Direct targets must exist in the same workspace, including offline targets. The receipt then adds `recipientOffline:true`; delivery waits for resume.
- Topic fanout snapshots active sessions subscribed to that exact topic in the workspace, excluding the sender.
- For direct messages, set `target` to the direct ID and `topic` to null; for topics, set `target` to the topic.
- Subscription replacement validates at most 32 topics. In one transaction it deletes only removed topics and inserts only new ones, so an identical replacement writes no audit rows.

Correlation:

- `conversationId` is optional, case-sensitive ASCII `[A-Za-z0-9._:-]`, 1–128 characters. `replyTo` is an optional positive safe-integer message ID. Roots default to null for both.
- A reply inherits its parent's nullable conversation ID. Explicit mismatches fail.
- The parent must exist in this workspace and be visible to the sender as its original sender or recipient. Expired or acknowledged parents stay valid.
- Only `complete` creates CLI/MCP replies: it resolves the parent sender and inherits conversation metadata inside the completion transaction. `send_message` rejects `replyTo`; progress uses an explicit recipient and conversation ID.
- Correlation grants no extra access to the parent.
- These fields are immutable, part of retry equality and `message.created` audit, and exposed through inbox/entity views and list filters.
- A direct SQL client that sets `replyTo` supplies the explicit target and inherited `conversationId`. Triggers enforce visibility and exact nullable conversation matching. Do not make a new conversation ID for a parent with null correlation.
- This metadata links ordinary messages. It does not define task completion, cancellation, authorization or a separate conversation entity.

Inbox:

- Joins messages and deliveries for the recipient. Requires an active session, `acknowledgedAt IS NULL`, and an unexpired message.
- Items add the sender's `senderName` and omit null `topic`, `conversationId` and `replyTo`.
- `inbox wait` skips the query while `PRAGMA data_version` shows no commit since the last empty read.
- Returns ascending message IDs, at most 100 and targeting 16 KiB of serialized JSON, with `next` when another row exists. Size-based pages use the last returned ID as the cursor, so no row is lost.
- Reads never acknowledge.
- After you process a page sequence, start a new poll from zero, so earlier unhandled messages are not skipped forever.

## One-time delivery and host hooks

Attach a known active identity with `attach`:

| Transport | Requires |
| --- | --- |
| `raw` | No vendor API |
| `claude` | The existing session's Unix inbox socket and vendor session ID |
| `codex` | Its owning app-server endpoint and thread ID |
| `grok` | The resident session UUID and a same-user Unix leader socket |
| `opencode` | An existing native session ID and `http://<literal-loopback-IP>:<port>` (no credentials, path, query or fragment; proxies and redirects disabled) |

- Unix sockets must belong to this OS user. Codex WebSocket TCP hosts must be loopback.
- Binding creates no model and imports no transcripts. Per-vendor wake, receipt and preflight behavior: [the service protocol](SERVICE_PROTOCOL.md#identity-and-capabilities).
- `dispatch` sends one bounded batch (Grok waits up to 300 s for ACP completion).
- `listen` keeps presence and checks commits every 250 ms while recently active, backing off to 1 s when idle, and reuses native connections.
- Raw listeners maintain presence; a host invokes `hook` to consume context.
- Every route uses the shared DB/audit; no sender model is needed.

The durable protocol:

1. In `BEGIN IMMEDIATE`, validate active recipient/workspace and select ascending unacknowledged, unexpired deliveries with no dispatch or state `ready`, excluding live claims. Limit to 16 messages, targeting 16 KiB; allow one larger row to make progress. Recheck after you acquire the writer.
2. Upsert `(message,recipient)` with a fresh UUID token, transport, state `staged`, and timestamp. Commit **before** external I/O. Concurrent consumers now skip it.
3. Send attributed peer data to the host. Store `submitted` on transport success, or `uncertain` on failure, guarded by the exact token and state `staged`.
4. The recipient calls `complete` after handling. Submission is never acknowledgement.

- Dispatch output can expose transport, receipt kind and `recipientTurnRequested`. `modelCalls:0` describes routing, not recipient inference.
- OpenCode's HTTP receipt and Codex's RPC response are transport-specific evidence. Claude records socket-write success only.
- Never convert a native failure into an automatic raw fallback: the native recipient may already have the message.
- `staged`, `submitted`, and `uncertain` are never automatically offered again, including after restart or resume. `inbox` still exposes unacknowledged messages for inspection/recovery.
- `retry_delivery` explicitly returns a pending unexpired recipient delivery to `ready` and records the reason. A retry of an uncertain send can duplicate context if it arrived before the connection failed.
- The design chooses no automatic replay over guaranteed delivery. Exactly-once external effects are not promised. Managed workers use this same dispatch state machine.

Hook formats:

- `hook {"format":"text"}` emits only new attributed context (nothing when empty).
- `format:"claude"` emits `UserPromptSubmit` additionalContext JSON; configure it on that event.
- `format:"json"` returns `{items,context}` for host adapters. `context` is the canonical compact rendering (empty when no items), so adapters need not duplicate prompt policy.
- Normally stdout write/flush marks submission: offered to the host, not host acceptance.
- SDK adapters use `{"format":"json","deferConfirm":true}`, queue the messages, then call `confirm_delivery` with `items:[{id,dispatchToken}]`. Confirmation is idempotent for that attempt; unknown/superseded tokens fail. A failed host queue leaves staged data for explicit recovery. Pi uses this two-step form.
- Hooks do not install themselves or wake arbitrary agents. A host must expose an injection event and consume stdout.
- Without hooks/APIs, any agent can run the CLI hook itself, or use `inbox`/`complete` through a conforming SQLite client. `inbox wait` polls every 250 ms for up to 60000 ms and requires separate presence maintenance.
- Never run a native attachment and a raw hook consumer for the same identity.

## Delivery scheduling and durable host receipts

Wake:

- `wake:"passive"` records information without requesting a managed model turn. `wake:"action"` requests a turn only within the receiver's already authorized task. Neither grants new authority.
- Managed idle dispatch checks for eligible actionable mail before staging, puts it ahead of passive backlog, and includes up to 16 messages within the byte budget.
- Passive-only mail stays unclaimed and unacknowledged until a later authorized turn or explicit inbox recovery.
- The initial user task excludes queued peer mail.
- Attached hosts keep their native scheduling behavior. Wake intent does not invent a host wake capability.

Pi (`registerPiInbox(pi, options)` returns `call(command,input)`, `getBinding()` and `drain()`):

- Options: binary/database/workspace/session, `enabled(ctx)`, `onBinding(binding|null)` and `disableCacheWarming` (default false).
- It registers stable tools synchronously once and resolves their binding per call. Disabled persistence creates no store.
- Host delivery uses `steer` with `triggerTurn:false` at idle boundaries. It checks the actual session-file JSONL receipt and matching header before it confirms.
- Pi may buffer a fresh session until its first assistant message. Those entries stay staged even when visible in memory. Tools and presence stay available.
- A missing or disabled session file never becomes a confirmed durable receipt.
- A lifecycle restart reuses the ledger identity and reconciles only its own `raw:pi:<vendorSession>` staged attempts: persisted tokens are confirmed; absent tokens are explicitly retried after complete ledger inspection; memory-only receipts stay staged until a disk receipt appears.
- A fresh session with no persisted host identity cannot promise automatic recipient identity recovery after process loss. Its messages stay in the database for explicit recovery.
- Submitted/uncertain attempts are never automatically retried. Run only one adapter lifecycle owner for a Pi session. Missing receipt support fails closed. No model call performs recovery.

`check_paths` (host-only CLI command):

- Accepts up to 128 `{path,kind?}` entries.
- Returns foreign live advisory lease conflicts (`id`, relative `path`, `kind`, `owner`, `expiresAt`, `reasoning`) with native canonicalization and indexed tree overlap, without acquisition or renewal, under one read snapshot.
- Reports at most 100 distinct conflicts and adds `truncated:true` when more exist.
- A clear result is a point-in-time observation, not permission to overwrite or an OS lock.

Activity file rows:

- Keep raw Git `status` and add `changes`, `indexState`, and `worktreeState` labels.
- Rename sources stay `previousPath`; they are not independent deletions.
- When time filtering hides entries with unknown mtimes, `coverage.unknownTimeQuery` is an executable scoped activity input without the time filter. Unknown mtimes stay null; absence from a filtered page does not prove deletion.

### Atomic final reply

- CLI/MCP/Pi `complete {message:ID,reply:"answer"}` commits a final direct reply and completion in one writer transaction.
- It validates that the message was delivered to this session, replies to its sender, and sets the parent delivery's `acknowledgedAt` only if NULL.
- The reserved sender-scoped `complete:<ID>` key makes identical retries idempotent; changed replies fail.
- A previously completed no-reply message cannot get a final reply later. All writes roll back on error.
- `send_message` sends questions/partial work without completion.
- `complete` without `reply` handles one message or an atomic batch of 1–100 unique received IDs.
- The `acknowledgedAt` column and audit events record completion.

### Reply requirements

- Every message exposes immutable `replyRequired`. Direct requests default to `true`; informational sends set `false`; topics/broadcasts default to `false` (explicit `true` is allowed). Replies are always informational.
- `complete` must supply a final reply for an unanswered required request, and must omit reply for informational messages.
- Batch completion rolls back if any request still needs an answer. Partial `send_message` replies do not satisfy completion.
- New follow-up work starts a new direct request; it cannot reply to an informational message.
- The runtime and SQLite enforce these rules, whatever the message wording or wake mode.
- A valid completion receipt proves the declared protocol was followed, not that a review finding is correct.

## Audit and usage

- SQLite triggers atomically log identity registration/changes, messages, fanout, acknowledgements, lease changes, subscriptions, attachments and dispatch attempts, also for conforming direct SQL clients. Ordinary heartbeats avoid event spam.
- Message bodies live once in immutable message rows; audit references their IDs. Audit rows reject update/delete.
- Identity strings are not authentication secrets. A process with filesystem access can still replace the database or schema.
- `record_usage` stores available host metrics once per session/key. A changed payload under the same key fails.
- Keep `scope`: `request` is one model request, `turn` is an aggregate result, `cumulative` is a session snapshot. Never sum cumulative snapshots or overlapping scopes.
- Cache reads and cache writes are separate counters. Input counters keep vendor semantics (Codex includes cache hits; Anthropic input excludes cache read/write tokens).
- Use `contextTokens` when available for the current request; do not infer it from an aggregate turn. Missing counters are unknown.
- Attached Pi and managed workers report available usage automatically. Native Claude/Codex injection does not expose the owner's inference usage: that host must call `record_usage`.
- Routing makes zero model calls and adds only new peer data, not the skill or full transcript on every delivery. Recipient history stays vendor-owned.

## Cleanup and conformance

Retention:

- Expiry does not depend on pruning. Each prune transaction removes up to 100 expired leases of the bound workspace (removal is audited). Repeat its continuation until no work remains.
- Sessions, messages, deliveries, dispatch receipts and audit are kept, including after acknowledgement/expiry. Expiry controls eligibility, not audit retention.
- There is no automatic transcript deletion or audit purge. Long-running stores need an operator-defined archive/retention policy. Do not delete the DB while workers run.
- `listen`, `dispatch` and `run` create an empty `<db>.owner-<sha16>.lock` beside the database per session (`sha16` is the first 16 hex digits of the session ID's SHA-256). They hold its OS advisory lock while they own delivery. The files carry no data, stay after exit, and are neither exported nor needed for restore; see [delivery ownership](SERVICE_PROTOCOL.md#identity-and-capabilities).

Export (`db export {"path":"/absolute/new.sqlite"} --database /absolute/source.sqlite`):

- Creates a consistent snapshot of the **entire trusted-user database**, across all workspaces. It includes committed WAL content, keeps the source, and needs no session.
- The destination must be a new absolute filename in an existing directory. Existing files/symlinks and symlink sources are rejected.
- Export writes a private temporary file, validates schema/integrity/foreign keys, syncs its contents, and publishes without overwriting a competing destination.
- Output: `path`, `source`, `schemaVersion`, SHA-256, byte count, `integrity`, `directorySynced`, `scope:"all-workspaces"` and `includesWorkspaceDocuments:false`. A false `directorySynced` reports a durability limit of an otherwise published file.
- The snapshot contains document audit metadata, **not** external files under each workspace's `.octocode/communication/`. Keep those files separately and verify registered hashes.
- It is a point-in-time export, not a continuous replica, workspace-filtered export, retention purge or multi-file atomic backup. Changes committed after its read snapshot are outside it.

Restore:

1. Stop all readers/writers.
2. Keep the existing database plus any WAL/SHM companions and document files.
3. Verify the export hash and schema.
4. Prefer to reopen the exported database at its separate path, with all clients bound to that path. Inspect state before you resume workers.

- Never replace a live database or mix unrelated WAL files with a restored snapshot.
- There is no restore command, automatic history merge or in-place schema downgrade. Export does not relax exact schema compatibility or authorize deletion of the original history.

Conformance: a process that cannot exec the CLI follows this protocol itself. The skill ships no second client. Initialize the store with the CLI, then keep the same identity, expiry, transaction, idempotency, and acknowledgement rules.
