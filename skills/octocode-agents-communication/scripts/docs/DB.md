# Local database protocol

The coordination contract for cooperating local processes. Prefer the bundled Python CLI; a generic agent can use SQLite alone if it follows the same transactions and expiry rules. DB writes persist messages; a deterministic dispatcher or host hook delivers notifications. Every agent joins before it communicates, and replies use this same store.

## Identity and discovery

- Default path: `<Octocode home>/agents-communication/communication.sqlite`, resolved by `@octocodeai/config` (shared Python helper copied into the bundle); `--database` overrides it.
- `db info` reports actual path, canonical workspace, existence, application ID, generation, schema digest, SQLite version and journal mode; inspecting a missing file creates nothing.
- `schema` returns the command catalog, bound tools, entity catalog and canonical DDL under `database.sql`; `schema entities` and `schema entity <name>` narrow it.
- Application ID, schema version, digest and foreign keys derive from the single bundled `schema.sql`. The CLI checks every user table, index, constraint and foreign key through normalized `sqlite_schema`, never fills in missing tables, and rejects any other schema. Only the current schema exists: no legacy schemas or migrations.
- Direct sends default to `wake:"action"`, fanout to `passive`; SQL writers supply `wake` explicitly.
- Session `task`/`status`, workspace revision counters and receiver views support discovery; they own no messages or delivery state. Ordinary heartbeats leave the directory revision unchanged; expiry uses the clock.

### Lookup indexes

| Index | Serves |
| --- | --- |
| `leases_path(workspace,pathKey)` | lock/lock_many, `check_paths`, lease path filters |
| `leases_owner(owner)` | leave/resume cleanup, held-lease lists |
| `sessions_scope(workspace,expiresAt)` | peers, notify_all and topic recipients |
| `sessions_host(workspace,vendorSession)` partial | host identity binding |
| `subscriptions_topic(topic)` | topic fanout |
| `documents_scan(workspace,id)` + `UNIQUE(workspace,name)` | `context`, `read_document`, `share_document` |

Schema digest: SHA-256 of compact UTF-8 JSON (no ASCII escaping, column order kept) of `type,name,tbl_name,sql` rows from `sqlite_schema`, excluding names beginning `sqlite_`, ordered by `type,name`, each SQL whitespace-collapsed and trimmed. `schema` exposes the expected digest; `db info` the expected and observed ones.

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

- `agentIdField` values are DB agent UUIDs, not vendor session IDs, claim tokens or entity IDs; no duplicate `agentId` column exists.
- Message `target` (possibly a topic) is routing data, not a foreign key: use `sender` for authorship and delivery rows for recipients. Documents/context expose `author` from the publication audit's `session`. Peer views belong to `session`; the workspace peer revision counter has no owner.
- `database.relationships` lists every SQL foreign key, internal tables included; each entity exposes its subset as `relationships`. Composite keys keep column order: dispatch `(message,recipient)` → delivery `(message,recipient)`. Messages reference sender and optional parent; deliveries reference message and recipient; leases, subscriptions, attachments, audit and peer views reference sessions; documents reference their publication audit row.
- Get/list cover every public entity; set covers only entities with supported mutations.

Access:

- Every entity command needs `--session` of the bound workspace; get/list accepts an expired identity for inspection.
- Session, lease, subscription, attachment and audit metadata is workspace-visible. A message is visible to its sender and recipients, a delivery to its sender and recipient. Unknown or inaccessible IDs return JSON `null`.
- Rows (inbox, peers, entity views) omit SQL-null columns; lease rows never expose `pathKey`.
- Identity strings route; they are not secrets. This trusted OS-user database is no security boundary against a process with file access.

Get, set, list:

- Get returns one row (subscriptions aggregate into `{id,session,topics}`).
- Set accepts only your session's `name`, `task`, `status`, nullable `vendorSession`, or your subscription `topics`. Vendor and session ID are immutable; `entity set` cannot replace lease or message transitions.
- List returns at most 100 items, targeting 16 KiB of JSON, plus nullable `next: {command,input}` to run unchanged. One oversized row stays intact with a budget diagnostic, so lists progress.
- Integer IDs sort numerically; delivery/dispatch lists sort by `(message, recipient)` (`10:…` after `9:…`). Message, delivery and dispatch lists start from the caller's sent/received index; audit lists walk IDs to the page size.
- Session/lease lists default to `presence:active` (`expired` or `all` for history); lease activity includes owner presence. Lease `path` with optional `kind` lists overlaps by the acquisition rules, acquiring nothing.
- Filters: messages `direction`, `topic`, `conversationId`, `replyTo`; deliveries `message`, `acknowledged`; subscriptions exact `topic`. Full schemas: `schema entity <name>`.

## Connection and transactions

- Local filesystem, SQLite 3.42+ (the CLI uses its interpreter's SQLite).
- New stores use WAL on patched SQLite (3.51.3+, 3.50.7–3.50.x, 3.44.6–3.44.x), else DELETE journaling. Existing WAL stores need a patched interpreter for writes; the runtime never changes a journal mode.
- Open an initialized store read/write-existing, so a path typo creates no separate store; CLI `join` initializes an empty store under a writer transaction. New files get mode 0600, created directories 0700; existing permissions stay.
- Every writing connection: foreign keys on, 5000 ms busy timeout, the journal policy above, FULL synchronous. Use parameter binding.
- The CLI validates the schema fingerprint under a read snapshot and takes `BEGIN IMMEDIATE` only for initialization and state changes.
- Begin a short `BEGIN IMMEDIATE` before you check identity or read state that controls a write; commit the whole transition or roll back. Never hold a transaction while you wait for a model, process or filesystem edit. Retry busy errors with a bounded policy; a failed write is never success.

Limits:

- Timestamps are integer Unix milliseconds from the local wall clock; an expiry is active only while `expiresAt > now`, so clock changes affect lease duration.
- Names, vendors, topics, retry keys, vendor-session strings: non-whitespace, at most 256 UTF-16 code units. Message bodies: at most 16384. Paths: at most 4096.
- TTLs in integer ms: leases 1000–600000 per acquisition/renewal; messages 1000–86400000.

## Presence

- Join inserts UUID, canonical workspace, name, vendor, optional vendor-session ID and `expiresAt=now+60000`.
- Heartbeat (about every 15 seconds) validates the active session, extends it by 60000 ms atomically, may update the vendor-session ID, and never resurrects an expired identity. With `renewLeases:true` (managed hosts) the same transaction extends live owned leases to at least `now+60000`, keeping longer deadlines and never reviving expired leases; otherwise it changes presence only.
- Resume needs an expired or ended session with matching workspace and vendor. One transaction deletes old leases, clears unacknowledged delivery claims and extends presence; messages, subscriptions and identity metadata stay.
- Leave expires presence and deletes owned leases and subscriptions, also for an already-expired session. A crashed identity keeps subscriptions for resume; process death needs no cleanup for logical expiry.
- DB resume does not recreate vendor history: managed `run --session` starts a fresh vendor conversation; attached hosts own theirs.

## Required intent

- New `send_message`, `notify_all`, `lock` and `lock_many` inputs need `reasoning`: the concrete need, not a reasoning transcript. Nonblank, at most 512 UTF-8 bytes; a bundle shares one reason; no generated fallback.
- It is stored immutable and non-null on the message/lease. Inbox, raw/native delivery, Pi and host hooks expose message intent; lease inspection and conflicts expose the owner's intent; conflict handoff suggestions carry the requester's reason, keyed by lease plus intent.
- Message retries need identical reasoning as well as body, target and topic. Renew/release keep the original intent in audit.
- SQLite insert triggers require it for every client; SQL clients insert it with the other fields, and updates cannot rewrite it.

## Path leases

Canonicalization:

- Canonicalize workspace and candidate through real filesystem ancestors, including symlinks and dangling links. Resolve links before consuming later `..`; never collapse lexically first. Bound link traversal; propagate permission, cycle and traversal errors.
- Reject paths outside the workspace by the case-preserving resolved path; folded keys never authorize filesystem access. Nonexistent components stay logical, relative to the resolved ancestor.

Comparison:

- One conservative namespace for all filesystems: components compare by Unicode 16 canonical caseless matching, `NFD(casefold(NFD(text)))`, used only for equality and tree-prefix checks; stored/access paths stay as given. So `A.txt` and `a.txt` conflict even on case-sensitive volumes, composed/decomposed names alias before they exist, and `src` contains `SRC/file` but not `src-other`.
- `schema.database.leasePathComparison` reports the pinned Unicode versions; CLI and entity queries share these rules. Never use SQLite's ASCII-only NOCASE or lower().
- All participants use one schema and comparison policy; mixed old/new lock clients are unsupported. Older development DBs without lease timestamps are kept but rejected: use a fresh shared DB.
- Leases are cooperative reservations, not OS locks: they do not fence an uncooperative writer, alias hard links, or freeze directory/symlink topology. Use a tree lease to change directory structure; reacquire after a rename or symlink change.

Acquisition (one writer transaction):

1. Validate active ownership.
2. Find live leases (own expiry and owner presence both active) in this workspace whose `pathKey` conflicts. `pathKey` is `"/" + NFD(casefold(NFD(component)))` per component, root omitted.
3. Reject when keys match, a tree lease holds an ancestor key, or a tree candidate has leases in the key range `(key+"/", key+"0")`, your own leases included.
4. Else insert the lease with `pathKey` (a trigger rejects a missing key), acquisition/refresh timestamps and TTL (default 60000, maximum 600000); SQLite assigns the ID.

- Overlap is three indexed lookups on `(workspace,pathKey)` (exact, ancestor trees, tree range), shared by acquisition, `check_paths` and entity path filters; no call loads every lease.
- `locks {}` lists active workspace locks (owner ID, reason, `acquiredAt`, `refreshedAt`, `expiresAt`), excluding expired leases and expired owners; follow every continuation. `entity list lease '{"presence":"all"}'` is history, never current ownership.

Renew and unlock take `leaseId` and atomically check active session, ID, owner and unexpired lease. Renew applies the requested TTL from now and returns `refreshedAt` and `expiresAt` (set together; expiry within 600000 ms of refresh). A false result means stop editing and acquire a fresh lease; never reuse IDs or revive expired leases. Direct SQL that skips these checks can insert conflicting leases: SQLite serializes, but compliance is still required.

`lock_many` takes 1–32 `{path,kind?}` requests and optional `ttlMs`. It resolves and validates all paths, rejects overlaps within the set, checks every candidate, and inserts all leases in one writer transaction; a conflict inserts nothing. Each lease has its own ID. It keeps previously held leases: release those before you wait.

Conflicts:

- Results give workspace-relative paths, owner identity once, held lease IDs and `retryAfterMs` (earlier of lease expiry and owner presence expiry): a snapshot, not a promised grant, since the owner may renew.
- `next` proposes one sender-keyed question per conflicting lease: send it once, do independent work, retry after a reply or expiry. Self-conflicts get no suggestion: reuse the covering lease, or release and reacquire.
- No fairness queue, no force-steal, and no operation waits on another agent inside a transaction.
- `leave` frees leases at once; process death frees them logically at the earlier expiry; a stuck process that still heartbeats loses unrenewed leases (heartbeats extend presence only).
- `prune` removes expired leases, or leases of expired owners, in the bound workspace only.

Regression tests: `tests/leases.test.mjs` (concurrent reversed sets, no partial grants, alias overlap, stale IDs, stalled-owner expiry, leave, expired-owner cleanup) and `tests/storage-scale.test.mjs` (lock, conflict and `check_paths` latency with 10,000 leases and 100,000 audit rows).

## Shared handoff documents

`share_document {name,content,reasoning}`:

- Publishes up to 1 MiB UTF-8 at the canonical workspace's `.octocode/communication/<name>`. Names: lowercase filenames with digits, dots, dashes or underscores; no directories, traversal or symlink components. CLI JSON `-` reads bounded stdin for large content.
- Immutable through this API: identical retries return the original metadata; different content, reasoning or context metadata needs a new name.
- `document.created` audit stores name, path, author, reasoning, byte count and SHA-256; content lives once on disk. An audit trigger registers each name once per workspace in the immutable `documents` table (`id` = audit ID) for name lookups and `context` scans.
- The CLI writes and syncs the file before the writer lock; the transaction only renames and registers. A read snapshot answers identical retries.
- Reasoning: concrete intent, nonblank, at most 512 UTF-8 bytes, validated by the shared runtime for CLI/MCP/Pi. DB-only publishers implement the same contract (the generic audit table does not enforce it). Historical records stay readable without invented intent; republish under a new name to add it, and restart resident MCP/worker processes after a catalog rebuild. This is a command-contract change, not a schema change.

`read_document {name,offset?,limit?}` verifies the registered hash and returns UTF-8-aligned byte pages (default 8192, limit 4–16384) with `next: {command,input}`. Only active identities in that workspace read registered files. Send names and offsets in short messages, not full contents. DB-only clients read metadata from workspace-scoped audit and verify disk content against the hash; publish with the CLI or the same contract.

Durability: publication renames atomically without overwrite, then commits metadata in SQLite. These are separate durability domains: a crash can leave an unregistered file; keep it and publish under a new name. External edits/deletions fail integrity checks; nothing deletes or repairs automatically. Same-user malicious directory replacement is outside this cooperative contract.

## Messages and notifications

Send:

- Validates the sender's active session and exactly one target: a direct session ID (`topic` null) or an exact topic (`target` = topic).
- One writer transaction checks `(sender,key)` idempotency, inserts the immutable message and its delivery rows. A repeated key returns the existing receipt only when target, topic, body, reasoning, wake, resolved correlation and `ttlMs` match; changed content fails; a retry does not refresh expiry. Default TTL 3600000 ms.
- Direct targets must exist in the same workspace, offline included; the receipt then adds `recipientOffline:true` and delivery waits for resume.
- Topic fanout snapshots active sessions subscribed to that exact topic in the workspace, sender excluded. Subscription replacement takes at most 32 topics and in one transaction deletes only removed and inserts only new topics, so an identical replacement writes no audit.

`notify_all` snapshots every other active session in the workspace, ignoring subscriptions: in the message's writer transaction, select `id FROM sessions WHERE workspace=? AND expiresAt>? AND id<>?` and insert one delivery each. Encode it as `target="*"`, `topic=NULL` (UUID IDs keep `*` reserved). No backfill for late joiners or inactive/other-workspace peers. A sender/key retry returns the original receipt and count without widening the snapshot; zero recipients is a stored success. Broadcasts use the same inbox/complete protocol.

Correlation:

- `conversationId`: optional case-sensitive ASCII `[A-Za-z0-9._:-]`, 1–128 characters. `replyTo`: optional positive safe-integer message ID. Both null on roots.
- A reply inherits its parent's nullable conversation ID; explicit mismatches fail. The parent must exist in this workspace and be visible to the sender as original sender or recipient; expired or acknowledged parents stay valid. Correlation grants no extra access.
- Only `complete` creates CLI/MCP replies, resolving the parent sender and conversation inside the completion transaction. `send_message` rejects `replyTo`; progress uses an explicit recipient and conversation ID.
- The fields are immutable, part of retry equality and `message.created` audit, and exposed in inbox/entity views and filters.
- A direct SQL client that sets `replyTo` supplies the target and inherited `conversationId`; triggers enforce visibility and exact nullable matching. Never invent a conversation ID for a parent with null correlation. Correlation links messages only; it defines no completion, cancellation, authorization or conversation entity.

Inbox:

- Joins messages and deliveries for an active recipient where `acknowledgedAt IS NULL` and the message is unexpired. Items add `senderName` and omit null `topic`, `conversationId` and `replyTo`. Reads never acknowledge.
- Ascending message IDs, at most 100 and about 16 KiB of JSON, with `next` when more exist; size-based pages use the last returned ID as cursor, so no row is lost. After a page sequence, poll again from zero so earlier unhandled messages are not skipped.
- `inbox wait` skips the query while `PRAGMA data_version` shows no commit since the last empty read.

## One-time delivery and host hooks

`attach` binds a known active identity:

| Transport | Requires |
| --- | --- |
| `raw` | No vendor API |
| `claude` | The existing session's Unix inbox socket and vendor session ID |
| `codex` | Its owning app-server endpoint and thread ID |
| `grok` | The resident session UUID and a same-user Unix leader socket |
| `opencode` | An existing native session ID and `http://<literal-loopback-IP>:<port>` (no credentials, path, query or fragment; proxies and redirects disabled) |

- Unix sockets must belong to this OS user; Codex WebSocket TCP hosts must be loopback. Binding creates no model and imports no transcript. Per-vendor wake, receipt and preflight: [the service protocol](SERVICE_PROTOCOL.md#identity-and-capabilities).
- `dispatch` sends one bounded batch (Grok waits up to 300 s for ACP completion). `listen` keeps presence, checks commits every 250 ms while active (1 s when idle) and reuses native connections. Raw listeners keep presence only; a host calls `hook` for context. Every route uses the shared DB/audit; no sender model.

The durable protocol:

1. In `BEGIN IMMEDIATE`, validate active recipient/workspace; select ascending unacknowledged, unexpired deliveries with no dispatch or state `ready`, excluding live claims; at most 16 messages, about 16 KiB (one larger row may pass to make progress). Recheck after you hold the writer.
2. Upsert `(message,recipient)` with a fresh UUID token, transport, state `staged` and timestamp. Commit **before** external I/O; concurrent consumers now skip it.
3. Send attributed peer data. Store `submitted` on transport success or `uncertain` on failure, guarded by the exact token and state `staged`.
4. The recipient calls `complete` after handling. Submission is never acknowledgement.

- Dispatch output may show transport, receipt kind and `recipientTurnRequested`; `modelCalls:0` describes routing, not recipient inference. OpenCode HTTP receipts and Codex RPC responses are transport evidence; Claude records socket-write success only.
- Never turn a native failure into an automatic raw fallback: the recipient may already have the message.
- `staged`, `submitted` and `uncertain` are never offered again automatically, even after restart or resume; `inbox` still shows them for recovery. `retry_delivery` returns a pending unexpired delivery to `ready` with a recorded reason; retrying an uncertain send can duplicate context.
- No automatic replay is chosen over guaranteed delivery; exactly-once external effects are not promised. Managed workers use the same state machine.

Hook formats:

- `hook {"format":"text"}` emits only new attributed context (nothing when empty). `format:"claude"` emits `UserPromptSubmit` additionalContext JSON for that event. `format:"json"` returns `{items,context}`; `context` is the canonical compact rendering (empty without items), so adapters copy no prompt policy.
- Stdout write/flush normally marks submission: offered, not host-accepted. SDK adapters (Pi) use `{"format":"json","deferConfirm":true}`, queue the messages, then call `confirm_delivery` with `items:[{id,dispatchToken}]`: idempotent per attempt; unknown/superseded tokens fail; a failed host queue leaves staged data for explicit recovery.
- Hooks neither install themselves nor wake arbitrary agents; the host needs an injection event that consumes stdout. Without hooks/APIs, any agent runs the CLI hook itself or uses `inbox`/`complete` through a conforming SQLite client. `inbox wait` polls every 250 ms for up to 60000 ms and needs separate presence upkeep.
- Never run a native attachment and a raw hook consumer for one identity.

## Delivery scheduling and durable host receipts

Wake:

- `wake:"passive"` records information without a managed model turn; `wake:"action"` requests a turn only within the receiver's authorized task. Neither grants authority, and wake intent invents no host wake capability: attached hosts keep native scheduling.
- Managed idle dispatch checks for eligible action mail before staging, puts it ahead of passive backlog, and takes up to 16 messages within the byte budget. Passive-only mail stays unclaimed and unacknowledged until a later authorized turn or inbox recovery. The initial user task excludes queued peer mail.

Pi (`registerPiInbox(pi, options)` returns `call(command,input)`, `getBinding()` and `drain()`):

- Options: binary/database/workspace/session, `enabled(ctx)`, `onBinding(binding|null)`, `disableCacheWarming` (default false). Stable tools register once synchronously and bind per call; disabled persistence creates no store.
- Delivery uses `steer` with `triggerTurn:false` at idle boundaries and confirms only after it finds the session-file JSONL receipt with a matching header. Pi may buffer a fresh session until its first assistant message: those entries stay staged even when visible in memory, while tools and presence stay available. A missing or disabled session file never counts as a durable receipt.
- A lifecycle restart reuses the ledger identity and reconciles only its own `raw:pi:<vendorSession>` staged attempts: persisted tokens confirm; absent tokens retry explicitly after full ledger inspection; memory-only receipts stay staged until a disk receipt appears. A fresh session with no persisted host identity cannot recover recipient identity after process loss; its messages wait in the DB.
- Submitted/uncertain attempts never retry automatically. One adapter lifecycle owner per Pi session. Missing receipt support fails closed. No model call performs recovery.

`check_paths` (host-only CLI) takes up to 128 `{path,kind?}` entries and returns foreign live lease conflicts (`id`, relative `path`, `kind`, `owner`, `expiresAt`, `reasoning`) under one read snapshot, with native canonicalization and indexed tree overlap, acquiring and renewing nothing. At most 100 distinct conflicts, plus `truncated:true` when more exist. A clear result is a point-in-time observation, not overwrite permission or an OS lock.

Activity file rows keep raw Git `status` and add `changes`, `indexState` and `worktreeState`. Rename sources stay `previousPath`, not deletions. When a time filter hides entries with unknown mtimes, `coverage.unknownTimeQuery` is an executable scoped input without the filter; unknown mtimes stay null, and absence from a filtered page does not prove deletion.

### Atomic final reply

- `complete {message:ID,reply:"answer"}` (CLI/MCP/Pi) commits a final direct reply to the sender and the completion in one writer transaction. It checks the message was delivered to this session and sets the parent delivery's `acknowledgedAt` only if NULL. The reserved sender-scoped `complete:<ID>` key makes identical retries idempotent; changed replies fail. A message completed without reply cannot get one later. Errors roll back every write.
- `complete` without `reply` handles one message or an atomic batch of 1–100 unique received IDs. `acknowledgedAt` and audit events record completion. `send_message` carries questions/partial work without completion.

### Reply requirements

- Every message has immutable `replyRequired`: direct requests default `true`, informational sends set `false`, topics/broadcasts default `false` (explicit `true` allowed); replies are always informational.
- `complete` must supply a final reply for an unanswered required request and must omit it for informational messages. A batch rolls back if any request still needs an answer; partial `send_message` replies do not count. Follow-up work starts a new direct request, never a reply to an informational message.
- Runtime and SQLite enforce this regardless of wording or wake mode. A valid receipt proves protocol compliance, not that a review finding is correct.

## Audit and usage

- SQLite triggers atomically log identity registration/changes, messages, fanout, acknowledgements, lease changes, subscriptions, attachments and dispatch attempts, also for conforming SQL clients; ordinary heartbeats log nothing. Bodies live once in immutable message rows; audit references IDs and rejects update/delete. A process with file access can still replace the DB or schema.
- `record_usage` stores available host metrics once per session/key; a changed payload under the same key fails. Keep `scope`: `request` = one model request, `turn` = aggregate result, `cumulative` = session snapshot; never sum cumulative snapshots or overlapping scopes.
- Cache reads and writes are separate counters. Input counters keep vendor semantics (Codex includes cache hits; Anthropic input excludes cache read/write tokens). Use `contextTokens` for the current request when available, never inferred from a turn aggregate; missing counters are unknown.
- Attached Pi and managed workers report usage automatically. Native Claude/Codex injection cannot see the owner's inference usage: that host calls `record_usage`.
- Routing makes zero model calls and adds only new peer data, never the skill or full transcript; recipient history stays vendor-owned.

## Cleanup and conformance

Retention:

- Expiry does not depend on pruning. Each prune transaction removes up to 100 expired leases in the bound workspace (audited); repeat its continuation until done.
- Sessions, messages, deliveries, dispatch receipts and audit are kept after acknowledgement/expiry: expiry controls eligibility, not retention. No automatic transcript deletion or audit purge exists; long-running stores need an operator archive/retention policy. Never delete the DB while workers run.
- `listen`, `dispatch` and `run` create an empty `<db>.owner-<sha16>.lock` per session beside the DB (`sha16` = first 16 hex digits of the session ID's SHA-256) and hold its OS advisory lock while they own delivery; the kernel releases it on crash. The files hold no data, stay after exit, and are neither exported nor needed for restore ([delivery ownership](SERVICE_PROTOCOL.md#identity-and-capabilities)).

Export (`db export {"path":"/absolute/new.sqlite"} --database /absolute/source.sqlite`):

- A consistent snapshot of the **entire trusted-user database** (all workspaces, committed WAL content included); keeps the source; needs no session.
- Destination: a new absolute filename in an existing directory; existing files/symlinks and symlink sources are rejected. Export writes a private temp file, validates schema/integrity/foreign keys, syncs, and publishes without overwriting a competing destination.
- Output: `path`, `source`, `schemaVersion`, SHA-256, byte count, `integrity`, `directorySynced`, `scope:"all-workspaces"`, `includesWorkspaceDocuments:false`. False `directorySynced` reports a durability limit of an otherwise published file.
- Document audit metadata is included, **not** the files under each workspace's `.octocode/communication/`: keep those separately and verify their hashes. It is a point-in-time export, not a replica, workspace filter, retention purge or multi-file atomic backup.

Restore:

1. Stop all readers/writers.
2. Keep the existing database, any WAL/SHM companions and document files.
3. Verify the export hash and schema.
4. Prefer reopening the export at its own path with every client bound to it; inspect state before workers resume.

Never replace a live database or mix unrelated WAL files with a snapshot. No restore command, history merge or in-place downgrade exists; export neither relaxes exact schema compatibility nor authorizes deleting the original history.

Conformance: a process that cannot exec the CLI follows this protocol itself; the skill ships no second client. Initialize the store with the CLI, then keep the same identity, expiry, transaction, idempotency and acknowledgement rules.
