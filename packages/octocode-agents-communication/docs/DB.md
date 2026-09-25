# Local database protocol v2

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

`schema` returns the command catalog, nine bound tools, entity catalog, and canonical
DDL under `database.sql`. `schema entities` and `schema entity <name>` narrow the
output. The database identity is application ID `1329678147` (`0x4f414743`),
`user_version=2`. Readers and writers must reject another identity or schema.
The CLI checks all user tables, indexes, constraints, and foreign-key declarations
through the normalized `sqlite_schema` definition; it never fills in missing tables.
The historical filename `v1.sqlite` stays stable; it does not indicate the current
schema version. There is no automatic migration or Awareness import. To upgrade an
intact v1 store, stop all workers and let presence expire, then run `db migrate`.
Migration checks the exact v1 schema, preserves identities/messages/deliveries, and
adds audit snapshots for existing sessions. It rejects active workers. Upgrade all
clients together; old schema-pinned clients must not keep writing.

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
as acquisition, without acquiring anything. Message lists support `direction` and
`topic`; delivery lists support `message` and `acknowledged`. Subscriptions support
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
the existing receipt only when target, topic, and body match; changed content fails.
A retry does not refresh expiry. Default message TTL is 3600000 ms.

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
requires its owning app-server endpoint and thread ID. Endpoints are local: Unix
sockets must belong to this OS user, and WebSocket TCP hosts must be loopback.
Binding an identity does not create a model or import vendor transcripts.

`dispatch` sends one bounded batch; `listen` keeps presence and polls every 250 ms,
reusing the Codex connection. Raw listeners maintain presence; a host invokes
`hook` to consume context. Pi's bundled `pi-inbox.mjs` calls the hook every second
and inserts new messages with native `pi.sendMessage`, without starting a turn.
Codex uses `thread/inject_items` without a turn. Claude's receiving session may
start generation according to its inbound policy. No sender model is needed.

The durable protocol is:

1. In `BEGIN IMMEDIATE`, validate active recipient/workspace and select ascending
   unacknowledged, unexpired deliveries with no dispatch or state `ready`, excluding
   live legacy claims. Limit to four messages, targeting 16 KiB; allow one larger
   row to make progress. Recheck after acquiring the writer.
2. Upsert `(message,recipient)` with a fresh UUID token, transport, state `staged`,
   and timestamp. Commit **before** external I/O. Concurrent consumers now skip it.
3. Send attributed peer data to the host. Store `submitted` on transport success,
   or `uncertain` on failure, guarded by the exact token and state `staged`.
4. The recipient calls `ack` after handling. Submission is never acknowledgement.

`staged`, `submitted`, and `uncertain` are never automatically offered again,
including after restart or resume. `inbox` still exposes unacknowledged messages
for inspection/recovery. `retry_delivery` explicitly returns a pending unexpired
recipient delivery to `ready` and records the reason. Retrying an uncertain send
can duplicate context if it arrived before the connection failed. This deliberately
chooses no automatic replay over guaranteed delivery; exactly-once external effects
are not promised. Managed workers use this same dispatch state machine.

`hook {"format":"text"}` emits only new attributed context (nothing when empty).
`format:"claude"` emits `UserPromptSubmit` additionalContext JSON; configure it on
that event. `format:"json"` returns `{items}` for host adapters. Normally stdout
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
snapshots or overlapping scopes. Missing counters are unknown. Attached Pi and
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

The [Python example](../skills/octocode-agents-communication/scripts/sqlite_agent.py) implements direct messages, notify_all,
presence, resume, acknowledgements, and path leases using only the standard library.
It opens a store initialized by the CLI and pins the v2 schema digest. Path leases
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
