# Local database protocol v6

This reference defines the coordination contract for cooperating local processes.
Use the bundled Rust CLI when available. A generic agent can use
SQLite alone if it follows the same transactions and expiry rules. Database writes
persist messages; a deterministic dispatcher or host hook delivers notifications.
Every participating agent joins before communicating. Replies use this same store.

## Identity and discovery

The default path is `<Octocode home>/agents-communication/v1.sqlite`, resolved by
`@octocodeai/config` (the native helper is compiled into the binary). `--database` overrides it. Run `db info` to discover the actual
path, canonical workspace, existence, application ID, generation, schema digest,
SQLite version, and journal mode. Missing-file inspection does not create a store.

`schema` returns the command catalog, thirteen bound tools, entity catalog, and canonical
DDL under `database.sql`. `schema entities` and `schema entity <name>` narrow the
output. The database identity is application ID `1329678147` (`0x4f414743`),
`user_version=6`. Readers and writers must reject another identity or schema.
The CLI checks all user tables, indexes, constraints, and foreign-key declarations
through the normalized `sqlite_schema` definition; it never fills in missing tables.
The historical filename `v1.sqlite` stays stable; it does not indicate the current
schema version. There is no automatic migration or Awareness import. To upgrade an
intact v1, v2, v3, v4 or v5 store, stop all workers and let presence expire, then run `db migrate`.
Migration checks the exact old schema, preserves identities/messages/deliveries,
and adds audit snapshots when upgrading v1. V3 adds intent without inventing reasons
for old rows: historical `reasoning` remains null. It rejects active workers. Upgrade all
clients together; old schema-pinned clients must not keep writing. V4 adds immutable `wake` scheduling intent: old direct messages retain `action`, old topic/broadcast messages become `passive`. New CLI/Python direct sends default `action`; fanout defaults `passive`. SQL writers supply the corresponding value explicitly. V5 adds optional immutable conversation/reply correlation and OpenCode attachments; historical messages retain null correlation. V6 adds Grok attachments without changing message or audit history.

The schema digest is SHA-256 of UTF-8 JSON for `type,name,tbl_name,sql` rows from
`sqlite_schema`, excluding names beginning `sqlite_`, ordered by `type,name`.
Normalize each SQL definition by collapsing whitespace and trimming. Preserve column
order and use compact JSON without ASCII escaping. `schema` exposes the expected
digest; `db info` exposes expected and observed digests.

## Entities and access

| CLI name | Table | Identifier | Mutable fields or transitions |
| --- | --- | --- | --- |
| `session` | `sessions` | UUID | `name`, nullable `vendorSession`; join, heartbeat, resume, leave |
| `lease` | `leases` | Monotonic acquisition ID | Lock, renew, unlock |
| `message` | `messages` | Monotonic message ID | Send once; content is immutable |
| `delivery` | `deliveries` | `<message>:<recipient>` | Claim and acknowledge |
| `subscriptions` | `subscriptions` | Session UUID for the topic collection | Replace `topics` |
| `attachment` | `attachments` | Session UUID | Explicit `attach` binding |
| `dispatch` | `dispatches` | `<message>:<recipient>` | Stage, submit, uncertain, explicit retry |
| `audit` | `audit` | Monotonic event ID | Append only; `record_usage` adds host telemetry |

Every entity command requires `--session`. Get/list accepts an expired identity for
inspection, but the identity must belong to the bound workspace. Session, lease,
subscription, attachment, and audit metadata is visible within that workspace. A message is visible
to its sender or a recipient; a delivery is visible to its sender or that delivery's
recipient. Unknown or inaccessible IDs return JSON `null`. Identity strings are
routing identifiers, not secrets: this is a trusted OS-user database, not a security
boundary against another process with file access.

Get returns one row (subscriptions aggregate into `{id,session,topics}`). Set accepts
only your session's `name` and nullable `vendorSession`, or your subscription
collection's `topics`. Vendor and session ID stay immutable. Lease and message
transitions cannot be replaced by `entity set`.

List returns at most 100 items, targeting 256 KiB of serialized JSON, and a nullable
`next` cursor. A single oversized row is returned to ensure progress. Copy `next` into the
same filter's `after` field until null. Session/lease lists default to `status:active`;
use `expired` or `all` for history. Lease activity includes owner presence. Lease
`path` with optional `kind` lists overlaps using the same canonical file/tree rules
as acquisition, without acquiring anything. Message lists support `direction`, `topic`, `conversationId` and `replyTo`; delivery lists support `message` and `acknowledged`. Subscriptions support
exact `topic`. Discover each complete filter schema with `schema entity <name>`.

## Connection and transactions

Use a local filesystem and SQLite 3.51.3 or later. The Rust CLI bundles SQLite; direct SQL clients must supply a compatible version. Open an initialized store in
read/write-existing mode to avoid creating a separate store after a path typo. The
CLI's `join` initializes an empty store under a writer transaction. New files use
mode 0600; directories it creates use mode 0700. Existing file permissions are kept.

For every writing connection, enable foreign keys, a 5000 ms busy timeout, WAL,
and FULL synchronous durability. Use parameter binding. Begin a short
`BEGIN IMMEDIATE` transaction before checking identity and reading any state that
controls a write. Commit the complete transition, or roll it back. Never hold a
transaction while waiting for a model, another process, or filesystem edits. Retry
busy errors with a bounded policy; never treat a failed write as successful.

Timestamps are integer Unix milliseconds from the local wall clock. An expiry is
active only while `expiresAt > now`. Clock adjustments affect lease duration. Names,
vendors, topics, retry keys, and vendor-session strings must contain non-whitespace
text and be at most 256 UTF-16 code units; message bodies at most 16384 and paths at
most 4096. Lease/message TTLs are integers from 1000 through 86400000 milliseconds.

## Presence

Join inserts a UUID, canonical workspace, name, vendor, optional vendor-session ID,
and `expiresAt=now+60000`. Heartbeat validates the active session and extends it by
60000 ms atomically; it can also update the vendor-session ID. A normal heartbeat
cannot resurrect an expired identity. Send heartbeats approximately every 15 seconds.

Resume requires an expired or ended session with matching workspace and vendor.
In one transaction, delete its old leases, clear its unacknowledged delivery claims,
and extend presence. Preserve messages, subscriptions, and identity metadata. Leave
expires presence and deletes owned leases. It can clean up an already-expired
session. A process death needs no cleanup for logical expiry. DB resume does not recreate vendor history. Optional managed `run --session` starts
a fresh vendor conversation; attached hosts retain ownership of their conversation.

## Required intent

Every new `send_message`, `notify_all`, `lock` and `lock_many` input requires
`reasoning`: a concrete explanation of why the action is needed, not a reasoning
transcript. Keep it nonblank and at most 512 UTF-8 bytes. A bundle shares one reason
across its paths. There is no generated fallback reason.

The message/lease stores this immutable field. Inbox, raw/native delivery, Pi and
host hooks expose message intent; lease inspection and conflicts expose the owner's
intent. Conflict handoff suggestions carry the requester's reason and key requests
by lease plus intent. Message retries require identical reasoning as well as body,
target and topic. Lease renew/release preserve the original intent in audit.

SQLite insert triggers require reasoning even for clients without this CLI. SQL
clients must insert the column with the other fields; updates cannot rewrite it.
Only migrated historical records may have null reasoning. Upgrade all clients and
stop live sessions before `db migrate`; do not relabel historical unknown intent.

## Path leases

Canonicalize the workspace and candidate path using real filesystem ancestors,
including symlinks and dangling links. Resolve links before consuming subsequent
`..` components; never lexically collapse a path before resolving its links.
Reject paths outside the workspace using the case-preserving resolved path.
Remaining nonexistent components are relative to the resolved ancestor.

Lease comparisons use one conservative namespace on every filesystem: compare
components using Unicode 16 canonical caseless matching, `NFD(casefold(NFD(text)))`.
Preserve stored/access paths; apply folding only to equality and tree-prefix checks.
This intentionally makes `A.txt` and `a.txt` conflict even on case-sensitive volumes,
and treats composed/decomposed Unicode names as aliases before they exist.
`src` contains `SRC/file` but not `src-other`. `schema.database.leasePathComparison`
reports the pinned Unicode versions. Rust, Python, and entity conflict queries must
use these same rules; do not use SQLite's ASCII-only NOCASE or lower().

This replaces the earlier case-sensitive comparison without changing the SQL
schema. Stop old workers and release their leases (or wait for owner expiry) before
upgrading every participant. Mixed old/new lock clients are not supported. Reacquire after a
rename or symlink change; hard-link aliases and filesystem mutation enforcement
are outside the advisory path contract.

Within one writer transaction, validate active ownership and load leases whose own
expiry and owner's presence are both active in this workspace. Reject acquisition
when paths match, an existing tree contains the candidate, or a candidate tree
contains an existing path. This includes overlap with your own leases. Otherwise
insert a lease with default TTL 60000 and let SQLite assign its acquisition ID.

Renew and unlock must atomically check the active session, acquisition ID, matching
owner, and unexpired lease. Renew uses the requested TTL from now. A false result
means stop editing and acquire a fresh lease. Never reuse IDs or revive an expired
lease. Direct SQL that skips these checks can insert conflicting leases: application
protocol compliance is required, even though SQLite serializes transactions.

`lock_many` accepts 1–32 `{path,kind?}` requests and one optional `ttlMs`.
Resolve and validate all paths first, reject overlaps within the requested set,
then check every candidate and insert all leases in one writer transaction. A
conflict inserts nothing. Returned leases have separate IDs for renew/unlock.
This does not release previously held leases: release those before waiting.

Conflict results include owner identity, held lease IDs, and `retryAfterMs` based
on the earlier of the lease expiry and current owner presence expiry. This is a
snapshot, not a guaranteed future grant: the owner may renew. `next` proposes one
sender-keyed question per conflicting lease; send it once, do independent work,
then retry after a reply or expiry. Self-conflicts have no message suggestion.
No operation waits for another agent while holding a transaction. Heartbeats
extend presence only. `leave` frees leases immediately; process death frees them
logically at the earlier expiry. A stuck but heartbeating process still loses
unrenewed leases. `prune` removes expired leases or leases of expired owners.

## Shared handoff documents

`share_document {name,content,reasoning}` publishes up to 1 MiB UTF-8 under the canonical
workspace's `.octocode/communication/<name>`. Use lowercase filenames with digits,
dots, dashes or underscores; no directories, traversal or symlink components.
CLI JSON `-` reads bounded stdin for content too large for command arguments.
The file is immutable through this API; identical retries return the original
metadata. Different content, reasoning or context metadata requires a new name. `document.created` audit data
stores name, path, author, reasoning, byte count and SHA-256; content stays once on disk.
Reasoning is concrete intent (nonblank, at most 512 UTF-8 bytes), validated by the
shared runtime for CLI/MCP/Pi. DB-only publishers must implement the same contract;
the generic audit table does not enforce document-specific fields. Historical records
remain readable without invented intent; republish under a new name to add intent.
Update publishers to supply intent and restart resident MCP/worker processes to
load the rebuilt catalog. This changes the unpublished command contract, not the DB schema.

`read_document {name,offset?,limit?}` verifies the registered hash and returns
UTF-8-aligned byte pages (default 8192 bytes, limit 4–16384), with a `next` input
object. Send document names and relevant offsets in short DB messages, not repeated
full contents. Only active identities in that workspace can read registered files.
DB-only clients can read metadata from workspace-scoped audit and verify disk
content against its hash; use the CLI to publish or implement the same contract.

File publication uses an atomic no-overwrite rename, then metadata is committed
in SQLite. These are separate durability domains: a crash can leave an unregistered
file; preserve it and publish under a new name. External edits/deletions fail
integrity checks. No automatic document deletion or repair occurs. Same-user
malicious directory replacement is outside this cooperative filesystem contract.

## Messages and notifications

`notify_all` snapshots every other active session in the current workspace, without
subscriptions. Within the same writer transaction as the message insert, select
`id FROM sessions WHERE workspace=? AND expiresAt>? AND id<>?` and insert one delivery
per selected ID. Encode this broadcast as `target="*"`, `topic=NULL`; session IDs are
UUIDs, so `*` is reserved. Late joiners and inactive/other-workspace peers get no
backfill. Retrying the sender/key returns the original receipt and recipient count,
without expanding its snapshot. Zero recipients is a successful stored broadcast.
Existing inbox/ack clients consume broadcasts through deliveries without a migration.

Send validates the sender's active session and exactly one target: direct session
ID or exact topic. In one writer transaction, check `(sender,key)` idempotency,
insert the immutable message, and insert its delivery rows. A repeated key returns
the existing receipt only when target, topic, body, reasoning, wake and resolved correlation match; changed content fails.
A retry does not refresh expiry. Default message TTL is 3600000 ms.

`conversationId` is optional, case-sensitive ASCII `[A-Za-z0-9._:-]`, 1–128
characters. `replyTo` is an optional positive safe-integer message ID. Roots default
to null for both fields. A reply inherits its parent's nullable conversation ID;
explicit mismatches fail. The parent must exist in this workspace and be visible
to the sender as its original sender or recipient. Expired or acknowledged parents
remain valid. CLI, bound tools and the Python helper infer the parent sender when
`replyTo` is supplied without `to` or `topic`; explicit targets remain authoritative.
Resolution and visibility checks share the writer transaction. A reply can target another peer, but correlation grants that peer no
access to the parent. These fields are immutable, included in retry equality and
`message.created` audit, and exposed through inbox/entity views and list filters.

A direct SQL client supplies the explicit target and inherited `conversationId` when
setting `replyTo`; triggers enforce visibility and exact nullable conversation
matching. Do not manufacture a new conversation ID for a parent with null
correlation. This metadata links ordinary messages; it does not define task
completion, cancellation, authorization or a separate conversation entity.

Direct targets must exist in the same workspace, including offline targets. Topic
fanout snapshots active sessions subscribed to that exact topic in the workspace,
excluding the sender. Set `target` to the direct ID or topic and set `topic` to null
for direct messages. Subscription replacement validates at most 32 topics, deletes
the old set, and inserts the deduplicated new set in one transaction.

Inbox joins messages and deliveries for the recipient, requiring an active session,
`acknowledgedAt IS NULL`, and unexpired message. Return ascending message IDs, at
most 100 and targeting 256 KiB of serialized JSON, with `next` when another row
exists. Size-based pages use the last returned ID as the cursor, so no row is lost. Reads never acknowledge. Start a new
poll from zero after processing a page sequence so previously unhandled messages
are not skipped forever.

## One-time delivery and host hooks

Attach a known active identity with `attach`: `raw` needs no vendor API; `claude`
requires the existing session's Unix inbox socket and vendor session ID; `codex`
requires its owning app-server endpoint and thread ID. `opencode` requires an
existing native session ID and `http://<literal-loopback-IP>:<port>` server endpoint.
OpenCode endpoint credentials, paths, query and fragment are rejected; environment
proxies and redirects are disabled. Authenticated OpenCode servers are not yet
supported. Unix sockets must belong to this OS user; Codex WebSocket TCP hosts must
be loopback.
Binding an identity does not create a model or import vendor transcripts.

`dispatch` sends one bounded batch; `listen` keeps presence and polls every 250 ms,
reusing native connections. Raw listeners maintain presence; a host invokes
`hook` to consume context. Pi's bundled `pi-inbox.mjs` consumes canonical hook
context with native `pi.sendMessage`: action wakes an idle recipient; passive
inserts context without a turn. Busy Pi recipients defer batches until idle.
Codex checks the owning loaded thread's status before staging. When idle, action
uses `turn/start` with the new content once; passive uses `thread/inject_items`.
A busy/unloaded thread leaves mail unstaged. A race after staging is uncertain,
not automatically retried. Claude passive-only mail stays in storage until action;
native inbound policy can wake, hold or refuse the socket submission.

`grok` requires the existing session UUID and a same-user Unix leader socket.
Its version-1 leader envelope carries ACP; the adapter verifies the protocol
before staging and queues `session/prompt` into an already resident session. It
never loads/resumes the session: those APIs can replace the owner's MCP settings.
No history is replayed. Passive-only mail stays in storage.
No leader, recipient or sender model is created by attachment/delivery. The owning
host's tools and permissions remain authoritative. A pending prompt stays staged
until ACP completion; listeners keep heartbeating while waiting. One-shot
`dispatch` waits up to 300 seconds for that receipt. Cancellation, lost connections
and timeout leave inspectable uncertainty; the recipient may still finish.

OpenCode passive uses `/message` with `noReply:true` and verifies native IDs;
action uses `/prompt_async` and requires HTTP 204. Before staging, session/workspace
validation and status checks keep invalid or busy recipients queued. Authentication
stays in the listener environment, scoped to its endpoint, never in this DB.
Calls have a five-second deadline. A status race after staging still requires
inspection before retry. See [setup](../README.md#connect-a-native-recipient).
Every route uses the shared DB/audit. No sender model is needed.

The durable protocol is:

1. In `BEGIN IMMEDIATE`, validate active recipient/workspace and select ascending
   unacknowledged, unexpired deliveries with no dispatch or state `ready`, excluding
   live legacy claims. Limit to 16 messages, targeting 16 KiB; allow one larger
   row to make progress. Recheck after acquiring the writer.
2. Upsert `(message,recipient)` with a fresh UUID token, transport, state `staged`,
   and timestamp. Commit **before** external I/O. Concurrent consumers now skip it.
3. Send attributed peer data to the host. Store `submitted` on transport success,
   or `uncertain` on failure, guarded by the exact token and state `staged`.
4. The recipient calls `ack` after handling. Submission is never acknowledgement.

Dispatch output can expose transport, receipt kind and `recipientTurnRequested`.
`modelCalls:0` describes routing, not recipient inference. OpenCode's HTTP receipt
and Codex's RPC response are transport-specific evidence; Claude records
socket-write success only. Never convert a native failure into an automatic raw
fallback: the native recipient may already have received the message.

`staged`, `submitted`, and `uncertain` are never automatically offered again,
including after restart or resume. `inbox` still exposes unacknowledged messages
for inspection/recovery. `retry_delivery` explicitly returns a pending unexpired
recipient delivery to `ready` and records the reason. Retrying an uncertain send
can duplicate context if it arrived before the connection failed. This deliberately
chooses no automatic replay over guaranteed delivery; exactly-once external effects
are not promised. Managed workers use this same dispatch state machine.

`hook {"format":"text"}` emits only new attributed context (nothing when empty).
`format:"claude"` emits `UserPromptSubmit` additionalContext JSON; configure it on
that event. `format:"json"` returns `{items,context}` for host adapters; `context` is the canonical
compact rendering (empty when no items), so adapters need not duplicate prompt policy. Normally stdout
write/flush marks submission, meaning offered to the host, not host acceptance.
For SDK adapters, use `{"format":"json","deferConfirm":true}`, queue the messages,
then call `confirm_delivery` with `items:[{id,dispatchToken}]`. Confirmation is
idempotent for that attempt; unknown/superseded tokens fail. A failed host queue
leaves staged data for explicit recovery. Pi uses this two-step form.

Hooks do not install themselves or wake arbitrary agents. A host must expose an
injection event and consume stdout. Without hooks/APIs, any agent can run the CLI
hook itself, or use `inbox`/`ack` through a conforming SQLite client. `inbox wait`
polls every 250 ms for up to 60000 ms and requires separate presence maintenance.
Never run a native attachment and raw hook consumer for the same identity.

## Audit and usage

SQLite triggers atomically log identity registration/changes, messages, fanout,
acknowledgements, lease changes, subscriptions, attachments and dispatch attempts.
This includes conforming direct SQL clients. Ordinary heartbeats avoid event spam.
Message bodies live once in immutable message rows; audit references their IDs.
Audit rows reject update/delete. Identity strings are not authentication secrets;
a process with filesystem access can still replace the database or schema.

`record_usage` stores available host metrics once per session/key; a changed payload
under the same key fails. Preserve `scope`: `request` is one model request, `turn`
is an aggregate result, and `cumulative` is a session snapshot. Never sum cumulative
snapshots or overlapping scopes. Cache reads and cache writes are separate counters.
Input counters retain vendor semantics (Codex includes cache hits; Anthropic input
excludes cache read/write tokens); use `contextTokens` when available for the current
request and do not infer it from an aggregate turn. Missing counters are unknown. Attached Pi and
managed workers report available usage automatically. Native Claude/Codex injection
does not expose the owner's inference usage: that host must call `record_usage`.
Routing itself makes zero model calls and adds only new peer data, not the skill
or full transcript on every delivery. Recipient history remains vendor-owned.

## Cleanup and conformance

Expiry does not depend on pruning. Each prune transaction removes up to 100 expired
leases (their removal is audited). Repeat its continuation until no work remains.
Sessions, messages, deliveries, dispatch receipts and audit are retained, including
after acknowledgement/expiry. Expiry controls eligibility, not audit retention.
There is no automatic transcript deletion or audit purge; long-running stores need
an operator-defined archive/retention policy. Do not delete the DB while workers run.

`db export {"path":"/absolute/new.sqlite"} --database /absolute/source.sqlite`
creates a consistent snapshot of the **entire trusted-user database**, across all
workspaces. It includes committed WAL content, preserves the source, and needs no
session. The destination must be a new absolute filename in an existing directory;
existing files/symlinks and symlink sources are rejected. Export writes a private
temporary file, validates schema/integrity/foreign keys, syncs its contents, and
publishes without overwriting a competing destination. The output reports `path`,
`source`, `schemaVersion`, SHA-256, byte count, `integrity`, `directorySynced`,
`scope:"all-workspaces"` and `includesWorkspaceDocuments:false`. A false
`directorySynced` reports a durability limitation of an otherwise published file.

The snapshot contains document audit metadata, **not** external files under each
workspace's `.octocode/communication/`. Preserve those files separately and verify
registered hashes. This is a point-in-time export, not a continuous replica,
workspace-filtered export, retention purge or multi-file atomic backup. Changes
committed after its read snapshot are outside the exported point in time.

To restore, stop all readers/writers and preserve the existing database plus any
WAL/SHM companions and document files. Verify the export hash and schema, then
prefer reopening the exported database at its separate path with all clients bound
to that path; inspect state before resuming workers. Never replace a live database
or mix unrelated WAL files with a restored snapshot. There is no restore command,
automatic history merge or in-place schema downgrade. Export does not relax exact
schema compatibility or authorize deletion of the original history.

The [Python example](../skills/octocode-agents-communication/scripts/sqlite_agent.py) implements direct messages, notify_all,
presence, resume, acknowledgements, and path leases using only the standard library.
It opens a store initialized by the CLI and pins the v6 schema digest. Path leases
require Python 3.14 with Unicode 16.0.0, matching the native tables. Other operations
can run on older Python versions with a sufficiently recent SQLite. Topic send,
notification claiming, entity APIs, and pruning remain documented protocol operations
outside that small example. Its conformance test checks two-way messaging,
idempotency, acknowledgements, resume, and conflicting Python/Rust lease acquisition.

From the monorepo root, run all deterministic checks including Python conformance:

```sh
COMMUNICATION_PYTHON=/absolute/python3.14-with-recent-sqlite \
  yarn workspace @octocodeai/octocode-agents-communication verify
```

Without `COMMUNICATION_PYTHON`, the test runner explicitly skips Python conformance.
The [model POC](https://github.com/bgauryy/octocode/tree/main/packages/octocode-agents-communication#verify) separately exercises real Haiku and Luna processes.

## Delivery scheduling and durable host receipts

`wake:"passive"` records information without requesting a managed model turn;
`wake:"action"` requests a turn only within the receiver's already authorized task.
Neither grants new authority. Managed idle dispatch checks for eligible actionable
mail before staging, prioritizes it ahead of passive backlog, and includes up to
16 messages within the existing byte budget. Passive-only mail remains unclaimed
and unacknowledged until a later authorized turn or explicit inbox recovery. The
initial user task may include pending passive mail. Attached hosts retain their
native scheduling behavior; wake intent does not invent a host wake capability.

Pi's embedded `registerPiInbox(pi, options)` returns `call(command,input)`,
`getBinding()` and `drain()`. Options include binary/database/workspace/session,
`enabled(ctx)`, `onBinding(binding|null)` and `disableCacheWarming` (default false).
It registers stable tools synchronously once and resolves their binding per call. Disabled
persistence creates no store. Its host delivery uses `steer` with
`triggerTurn:false` at idle boundaries and checks the actual session-file JSONL receipt and matching header before
confirming. Pi may buffer a fresh session until its first assistant message; those
entries remain staged even when visible in memory. Tools and presence remain available.
A missing or disabled session file never becomes a confirmed durable receipt. A lifecycle restart reuses the ledger identity and reconciles only its
own `raw:pi:<vendorSession>` staged attempts: persisted tokens are confirmed; absent
tokens are explicitly retried after complete ledger inspection; memory-only receipts remain staged until a disk
receipt appears. A fresh session with no persisted host identity cannot promise
automatic recipient identity recovery after process loss; its messages remain in
the database for explicit recovery. Submitted/uncertain
attempts are never automatically retried. Run only one adapter lifecycle owner for
a Pi session. Missing receipt support fails closed. No model call performs recovery.

`check_paths` is a host-only CLI command accepting up to 128 `{path,kind?}` entries.
It returns foreign live advisory lease conflicts using native canonicalization and
tree overlap without acquisition or renewal. A clear result is a point-in-time
observation, not permission to overwrite or an OS lock.

Activity file rows retain raw Git `status` and add `changes`, `indexState`, and
`worktreeState` labels. Rename sources stay `previousPath`; they are not independent
deletions. When time filtering hides entries with unknown mtimes,
`coverage.unknownTimeQuery` is an executable scoped activity input without the time
filter. Unknown mtimes stay null; absence from a filtered page does not prove deletion.

### Atomic final reply

CLI/MCP/Pi `send_message` supports `ackReply:true` for a completed direct reply.
This is a transaction option, not a message column or a new schema version. A
conforming SQL client validates that `replyTo` was delivered to this session and
that the reply targets the parent sender, persists/verifies the keyed reply and
its deliveries, then sets the parent delivery's `acknowledgedAt` only if NULL, in
one writer transaction. Roll back all operations on any error. The Python example
implements this path. Clarification/partial replies leave the flag unset; ordinary
ACK remains available for handled messages requiring no reply.
