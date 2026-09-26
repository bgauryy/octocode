# Operating the local communication service

Use one supervised delivery owner per identity. Native adapters and raw hooks are
explicit alternatives; restarting a process never authorizes replay of uncertain
messages. There is no routing model to keep alive.

## Compact diagnostics

```sh
agents-communication health '{"staleAfterMs":300000,"limit":25}' \
  --workspace /absolute/repo --database /absolute/communication.sqlite
```

`health` opens an existing DB read-only and returns workspace-scoped counts plus
a bounded issue page. It reads no message bodies or audit transcripts and sends
no vendor requests. No session registration is needed for an operator using the
same trusted OS account. Missing/incompatible storage fails rather than creating
a second store. It remains readable while another WAL connection owns the writer.

`status:clear` means no observed queue issue; it is not a vendor availability,
authentication, inference-quality or listener-liveness probe. `status:pending`
means active queued/submitted work without a diagnosed fault; normal processing
is not an error. Successful reads exit zero even when status is `attention`;
monitoring should inspect the JSON.
Passive mail waiting for an action is normal. Submitted mail still requires the
recipient's explicit handling ACK.

| Issue | Operator response |
| --- | --- |
| `uncertain` | Inspect the actual recipient and dispatch token before any explicit retry |
| `expiredAction` | Decide whether work is still needed; acknowledge a deliberate no-action decision or issue a new request |
| `offlineRecipient` | Check the recipient and delivery owner; deliberately resume without reviving old leases |
| `stalledOffer` | Inspect the process offering context and the recipient before recovery |
| `overdueHandling` | Check recipient progress; do not convert submission into an ACK or blindly resend |

The last two use `staleAfterMs`, default five minutes. It is an observation
threshold, not a lease extension, cancellation or replay timer. Counts cover all
unacknowledged deliveries in this workspace; `issues` defaults to 25 rows, maximum
100. Follow the returned `next.command` and `next.input` unchanged. The cursor
includes message and recipient so a broadcast cannot lose recipients between
pages. Each call is a new read snapshot; start another scan when state changes.

A host monitor may sample these small summaries and notify only when the issue
set changes. Do not inject the same status or message bodies on every tick, and
do not ask a model to poll. This package reports diagnostics; it does not install
a scheduler or send external notifications.

## Recovery and storage

Keep heartbeats at 15 seconds; presence expires at 60 seconds. Presence does not
renew path leases. Guarded edits must stop on expired identity, missing coverage
or failed renewal. See [host lease admission](HOST_LEASE_GUARDS.md) for the exact
structured-tool coverage and the unguarded shell/custom-tool boundary.

Before a migration or restore, stop delivery owners and confirm no other process
is using the DB. Upgrading a v6 store to v7 is one `db migrate` transaction: it adds
lookup indexes, backfills folded lease keys and the document registry, then refreshes
planner statistics (about 1 s for 425,000 audit rows and 5,500 leases on an M-series
laptop). `prune` only removes expired leases of the bound workspace. `db export` creates a verified non-overwriting snapshot including
committed WAL state. Preserve `.octocode/communication/` documents separately.
Use a new path for a recovery rehearsal; never overwrite a live store. Retained
message keys and audit rows are protocol history, not disposable log clutter.
See [retention and compaction](RETENTION.md) and [the SQL protocol](DB.md).
