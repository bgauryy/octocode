# Retention and storage maintenance

Message expiry controls eligibility for delivery. It does not authorize deletion.
Acknowledgement records handling; the message remains evidence and its sender/key
pair prevents a retry from creating another message. Audit rows are append-only
and reference the original message rather than duplicating its body. Replies also
depend on retained parent messages and delivery visibility.

Consequently, schema v7 has no automatic message, session, delivery, dispatch or
audit purge. Maintenance can report retained history, remove already-expired
leases through `prune`, compact unused SQLite pages, and create full snapshots.
It never drops triggers or bypasses idempotency to reduce storage.

## Inspect retention

```sh
agents-communication db retention '{"limit":100}' --database /absolute/store.sqlite
```

The command is read-only and covers every workspace in the selected database.
It returns storage sizes and a bounded page of aggregate message statistics:

- `before`: expiry cutoff in Unix milliseconds; defaults to 30 days ago. It must
  be nonnegative and no later than now. This is `messages.expiresAt`, not creation
  time or a command to delete older messages.
- `afterId`: exclusive message-ID cursor; defaults to zero.
- `limit`: 1–1,000 messages per page; defaults to 100.
- `expiredSettledMessages`: expired before the cutoff, no unacknowledged delivery
  and no staged/uncertain dispatch. These are retained, not deletion candidates.
- `unacknowledgedMessages` and `unresolvedDispatchMessages`: counts within this
  page, including expired messages. These counters can overlap.
- `next`: the next command/input, retaining the original cutoff; `null` at the end.

Counts describe the scanned message page, not the entire database. Following
pages uses separate read snapshots, so concurrent acknowledgements or sends can
change later pages. Bodies and audit payloads are never returned by this report.
`reusableBytes` is SQLite free-page space available for reuse or compaction.

## Compact unused pages

```sh
agents-communication db compact '{}' --database /absolute/store.sqlite
```

This explicit command runs SQLite `VACUUM` once. It does not change protocol
records or delete history. It checks the schema fingerprint, database integrity
and foreign keys afterward, then requests a passive WAL checkpoint. Source path
replacement and symlinks are rejected; the ordinary schema compatibility check
still applies.

SQLite manages writer exclusion; there is no unreliable check-then-act test of
whether agents are active. A competing writer can cause a bounded busy failure:
normal database opening uses its existing five-second busy limit; compaction
uses a two-second SQLite busy timeout after opening. Failed `VACUUM` is not
automatically retried. Busy limits do not cap the CPU/I/O time for rebuilding a
large database. `VACUUM` may temporarily require additional space comparable to
twice the database size. See [SQLite's VACUUM contract](https://www.sqlite.org/lang_vacuum.html).

The output distinguishes:

- `logicalReclaimedBytes`: nonnegative reduction in SQLite page count × page size.
- `reclaimedBytes`: nonnegative difference in the main database file's size.
- `before`/`after`: main file and WAL allocation, page count and free pages.
- `checkpoint`: SQLite's passive-checkpoint result.

WAL allocation, pinned reader snapshots and concurrent writers can mask actual
disk savings. Compaction does not force WAL truncation or wait for all readers to
release their snapshots. The values are storage observations, not a throughput
benchmark. [SQLite documents passive checkpoint behavior](https://www.sqlite.org/pragma.html#pragma_wal_checkpoint).

## Leases and archives

`prune` remains the only expiry-based deletion command: it removes up to 100 expired
leases (or leases of expired owners) in the bound `--workspace` and records their
removal in audit. It never touches another workspace's leases. `next` is the same
command while a full batch was removed, otherwise `null`. It does not remove old
messages or closed identities. `leave` already deletes an identity's leases and
subscriptions; a crashed identity keeps subscriptions for `resume`.

`db export '{"path":"/absolute/new-snapshot.sqlite"}'` already creates a verified,
compact, full-database snapshot without changing the live source or overwriting
an existing destination. It includes committed WAL data and every workspace.
Keep referenced `.octocode/communication` documents separately. Snapshot deletion
or replacement of the live DB is an operator decision; there is no implicit
archive rotation that could discard audit history.

### Why there is no purge or in-place archive command

A purge of closed sessions' "unreferenced" rows was evaluated and rejected. Every
candidate row is referenced by a protocol guarantee: sessions are foreign-key
parents of audit, messages, deliveries and attachments; audit rows reject
update/delete by trigger and anchor the v7 `documents` registry; message
`(sender,key)` rows are the idempotency record a late retry is checked against;
`replyTo` visibility checks read retained parents and deliveries. Deleting any of
them would either require dropping integrity triggers or silently change what a
retry, reply or audit query returns, and a partial purge could not be verified by
the schema fingerprint. Growth is instead bounded operationally: `db export`
takes a verified snapshot, and an operator may start a fresh store at a new path
after all workers stop. Reads stay fast as history grows because hot paths use
indexes (see [DB.md](DB.md#schema-v7-indexes)); `db retention` measures what is kept.

## Verification

`node --test tests/retention.test.mjs` covers bounded pagination, cutoff validation,
missing databases, settled/pending/uncertain/correlated records, and writer-lock
failures. A compaction fixture inserts 400 expired leases, prunes them through the
real CLI, then verifies that compaction reduces logical pages while preserving
every remaining row—including audit, message bodies, idempotency keys, reply
relationships, dispatch states and protocol IDs. Additional checks reject schema
drift and symlink sources. All maintenance checks use disposable databases.

In the local release run on 2026-09-25, the 400-lease fixture shrank from
3,481,600 to 1,884,160 logical/main-file bytes (1,597,440 bytes reclaimed), retained
all 808 audit rows, and completed the CLI compaction call in 35.7 ms. The test
prints fresh measurements on each run. This small fixture demonstrates reclaim
and preservation; it does not predict large-database latency.
