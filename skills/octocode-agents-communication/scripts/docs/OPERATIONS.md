# Operations, recovery and retention

Use one supervised delivery owner per identity. Native adapters and raw hooks are
explicit alternatives; a restart never authorizes replay of uncertain messages.

## Health

```sh
agents-communication health '{"staleAfterMs":300000,"limit":25}' \
  --workspace /absolute/repo --database /absolute/communication.sqlite
```

`health` opens an existing DB read-only (no session needed, readable during a WAL
writer) and returns workspace-scoped counts plus a bounded issue page. It reads no
bodies or audit and calls no vendor. Missing/incompatible storage fails; it never
creates a second store. Reads exit zero even for `attention`; monitors inspect JSON.
`clear` means no observed queue issue, not vendor/auth/listener liveness. `pending`
means queued/submitted work without a fault. Passive mail waiting for action is normal.

| Issue | Operator response |
| --- | --- |
| `uncertain` | Inspect the actual recipient and dispatch token before any explicit retry |
| `expiredAction` | Acknowledge a deliberate no-action decision or issue a new request |
| `offlineRecipient` | Check recipient and delivery owner; resume without reviving old leases |
| `stalledOffer` | Inspect the offering process and the recipient before recovery |
| `overdueHandling` | Check recipient progress; never convert submission into completion or resend blindly |

The last two use `staleAfterMs` (default 5 min): an observation threshold, not a
lease extension or replay timer. `issues` defaults to 25 rows (max 100). Follow
`next.command`/`next.input` unchanged; the cursor includes message and recipient so
broadcasts lose no recipients. Each page is a new snapshot. Monitors notify only
when the issue set changes; never ask a model to poll. No scheduler is installed.

## Recovery

Heartbeat every 15 s; presence expires at 60 s. Plain heartbeats do not renew leases;
managed MCP, `run` and Pi do ([host setup](HOST_SETUP.md)). Guarded edits stop on
expired identity, missing coverage or failed renewal ([guards](HOST_LEASE_GUARDS.md)).
Before restoring, stop delivery owners and confirm no process uses the DB. Rehearse
recovery at a new path; never overwrite a live store.

## Retention

Expiry controls delivery eligibility, not deletion. Messages, `(sender,key)`
idempotency rows, replies' parents, deliveries, sessions and append-only audit are
protocol history: the schema has no purge, and maintenance never drops triggers.
To bound growth, `db export` a snapshot and start a fresh store at a new path after
all workers stop. Hot paths use [indexes](DB.md#lookup-indexes).

- `db retention '{"limit":100}' --database <db>`: read-only, all workspaces. Inputs:
  `before` (expiry cutoff ms, default 30 days ago, not in the future), `afterId`
  cursor, `limit` 1–1000. Reports storage sizes, `reusableBytes`, and per-page
  `expiredSettledMessages` (retained, not deletion candidates),
  `unacknowledgedMessages`, `unresolvedDispatchMessages` (may overlap), and `next`.
  Counts cover the scanned page; never returns bodies.
- `db compact '{}' --database <db>`: one `VACUUM`; checks schema fingerprint,
  integrity and foreign keys, then a passive WAL checkpoint. Rejects symlink sources.
  Busy timeout 2 s after opening (5 s to open); no automatic retry. May need about
  2x DB size temporarily. Reports `logicalReclaimedBytes`, `reclaimedBytes`,
  `before`/`after` and `checkpoint`; WAL and pinned readers can mask savings.
- `prune`: the only expiry-based deletion. Removes up to 100 expired leases (or
  leases of expired owners) in the bound workspace, audited; `next` repeats while a
  full batch was removed. `leave` deletes an identity's leases and subscriptions; a
  crashed identity keeps subscriptions for `resume`.
- `db export '{"path":"/absolute/new.sqlite"}'`: verified, non-overwriting full
  snapshot of all workspaces including committed WAL. It excludes
  `.octocode/communication/` documents; back them up separately. See
  [DB export](DB.md#cleanup-and-conformance).

Regression tests: `node --test tests/retention.test.mjs` (bounded pagination, cutoff
validation, missing databases, record states, writer-lock failures, compaction).
