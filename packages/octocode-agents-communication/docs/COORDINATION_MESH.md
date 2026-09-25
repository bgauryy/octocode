# Six-worker coordination check

Historical live results below precede required `reasoning` (schema v3). The current
CLI/DB intent contract and migration checks are documented in [DB.md](DB.md#required-intent).

`scripts/mesh-poc.mjs` launches two real Codex workers, two Claude workers and two
Pi workers through the copied skill. A controller and an independent Python SQLite
client share their workspace and database. No model routes messages.

Run from this package after building the skill. Set `COMMUNICATION_PYTHON` to a
Python interpreter and `COMMUNICATION_PI_MODEL` to an authenticated Pi provider/model:

```sh
COMMUNICATION_WORKERS_PER_VENDOR=2 node scripts/mesh-poc.mjs
```

Defaults are Codex `gpt-6-luna`, Claude `haiku`, and the selected Pi model. The
harness prints its temporary evidence directory and writes `result.json` on success
or `failure.json` on failure. It never commits, changes user vendor configuration,
or edits repository source through model tools. Do not replace the bundled binary
while the probe is running.

`COMMUNICATION_SKILL_PATH=/absolute/path/to/built/skill` uses an existing built
skill instead of copying it; the report records which mode ran. This is useful
when macOS stalls a newly copied executable in `_dyld_start` before application
code executes. Standalone-copy behavior remains separately covered by the CLI
tests. Each controller CLI operation has a two-minute process deadline.

## Acceptance gates

| Flow | Required evidence |
| --- | --- |
| Workspace awareness | Every worker calls `peers`, sees the DB-only participant, and answers a capability question with every supplied bound tool. |
| Shared context | The controller publishes a roughly 48 KB document under `.octocode/communication`; each worker reads only its requested 100-byte section and returns the unique proof line. Tool receipts must contain that proof. |
| Direct communication | All 30 directed worker pairs exchange a message and reply, including same-vendor pairs. Each original send has both a DB row and an actual vendor tool receipt. |
| Broadcast | Six `notify_all` calls each produce seven deliveries, with replies from the other five workers and DB-only participant. No topic subscriptions are created. |
| Generic agents | A Python client with no vendor executable on PATH sends direct messages to every worker and broadcasts; retry returns the same stored receipt and recipient snapshot. |
| Conflict collaboration | Each worker encounters the controller's parent-tree lease, asks the actual owner once, and finishes its turn. After release, each acquires, renews and releases the file in sequence. |
| Multiple paths | Each worker acquires a reverse-ordered pair through atomic `lock_many`, then releases both returned IDs. |
| Stalled owner | A present DB-only owner stops renewing a one-second lease. A peer acquires after expiry; the stale ID cannot renew or unlock the replacement. |
| Closed worker | A real Codex worker holds a long lease, then its managed process receives SIGTERM. Presence expires, its lease is released, and the controller immediately acquires the same path. |
| Completion | Every delivery is acknowledged. Cleanup checks zero leases, no active test sessions, and no surviving owned vendor processes. |

The closed-worker test is orderly termination. It does not claim to test a hard
crash, OS suspension or arbitrary filesystem writes by an uncooperative process.
Atomic bundle rollback and stale-owner exclusion are also covered by deterministic
runtime tests; the live mesh proves the real vendor adapters can invoke these tools.

## Efficiency measurements

The report includes message and delivery counts, maximum body bytes, shared-document
bytes omitted from each recipient's context, and direct-message reply latency from
DB audit timestamps. Reply latency includes the receiver model's queue and inference;
it is not raw transport latency.

Usage remains per worker. Codex cumulative reports use the latest snapshot instead
of being added repeatedly. Turn/request reports are summed for the other vendors.
Missing metrics remain null, and vendor token accounting is not treated as identical.
The report preserves raw usage audit rows so results can be checked independently.
A model interrupted during shutdown may not publish its final usage report.

Only new message IDs are dispatched to recipient turns. The worker's existing
conversation still consumes context; one-time dispatch does not erase vendor
history. Large context belongs in a document, and the recipient chooses the section
it needs. The harness reads a unique nonce from the document rather than putting
that nonce proof in the receiver's initial prompt.

Historical earlier meshes and their bounded ratings remain in
[MESH_REVIEW.md](MESH_REVIEW.md).

## First-run finding

The first expanded run completed every functional flow in 242.549 seconds with
207 messages, 249 acknowledged deliveries and no leftover leases or active
sessions. Its final automatic-delivery assertion correctly failed: Claude workers
had read 23 deliveries through `inbox` before the dispatcher staged them. There
were no uncertain dispatches or lost messages, but these unsolicited recovery reads
added context and made the automatic-delivery proof incomplete.

The managed worker instructions now explicitly reserve `inbox` for requested
recovery. Ordinary managed delivery comes from the host. The rerun retains the
strict requirement that every worker delivery has one submitted dispatch, rather
than weakening it to accept polling. The Python participant continues to test the
manual DB-only path. The first outcome remains recorded in
[coordination-mesh-first-attempt.json](../out/coordination-mesh-first-attempt.json).

## Verified rerun — September 25, 2026

The strict rerun **passed every gate in 523.297 seconds** with two Codex Luna,
two Claude Haiku and two Pi Haiku workers. Evidence:
[coordination-mesh-final.json](../out/coordination-mesh-final.json), token
`f6d79598-fb01-4a7d-bc69-3da14821c5c4`, skill SHA-256
`59533cf9f28a4e2d66e336acd6e2c17964935930917d86dbbaeb6a1fae887e96`.
This run used the existing built skill; standalone-copy execution is a separate
test because two copied-binary launches stalled in the macOS loader before main.

- **207 messages; 249 acknowledged deliveries.** All 30 directed worker pairs
  received real replies. Six worker broadcasts produced 42 deliveries, and the
  DB-only client also communicated with every worker.
- **Zero unsolicited inbox reads, automatic replays, unexpected model reports
  or supervisor corrections.** Every worker delivery had exactly one staged audit
  entry and a submitted dispatch. Controller and raw Python clients read their
  own inboxes directly, as intended.
- **Six owner questions and six lease lifecycles passed**, followed by six atomic
  two-path acquisitions. The present but stalled raw owner lost its expired lease;
  stale renewal/unlock failed. A real Codex worker closed while holding a long lease,
  and another owner immediately acquired it. Cleanup found no active identities,
  leases or surviving owned vendor processes.
- A **47,447-byte document** stayed in `.octocode/communication`. Every worker read
  only the final 47-byte proof through a 100-byte bounded request. Message bodies
  were at most **500 bytes**; the report conservatively counts at least 47,347
  document bytes avoided per recipient, excluding envelope overhead.
- Direct-message round-trip latency was **10.157 s median, 24.263 s p95**
  (3.373–25.928 s), including recipient inference and queuing.

Pi 2 paused for **290.498 seconds** between a `turn_start` and the next
`message_start`, after it had sent its conflict question. No lease was held, other
workers continued, and Pi recovered without intervention or replay. The trace proves
a vendor-turn stall; it does not establish the underlying network/provider cause.
Future harness runs persist `events.jsonl` and report pending-message stalls after
60 seconds without generating model prompts or replaying deliveries. This remains
a latency limitation even though the full acceptance run passed.

### Recorded recipient usage

These per-worker totals apply each vendor's own scope rules. They are workload
measurements, not comparable vendor billing totals.
Codex uses each worker's latest cumulative snapshot; Claude sums 14 per-turn results
per worker; Pi sums 46 request records per worker. Claude's output sequence decreases
between turns (for example 946, 844, 994, 2,028, 1,558), confirming it is not a
monotonically cumulative counter.

| Worker | Reported input | Cached input | Cache write | Output | Largest observed request context |
| --- | ---: | ---: | ---: | ---: | ---: |
| Codex 1 | 846,837 | 783,872 | Unknown | 3,353 | 20,873 |
| Codex 2 | 714,152 | 652,032 | Unknown | 3,313 | 19,630 |
| Claude 1 | 316 | 741,564 | 31,837 | 15,405 | Not exposed by turn totals |
| Claude 2 | 308 | 731,833 | 31,879 | 15,479 | Not exposed by turn totals |
| Pi 1 | 224 | 669,968 | 21,076 | 7,042 | 21,083 |
| Pi 2 | 221 | 647,267 | 21,476 | 7,566 | 21,481 |

Codex reported input includes its cached-input subset. Claude/Pi expose cache reads
and writes separately from uncached input. Do not add these rows into one universal
"input tokens" total. Recipient history still grows: one-time delivery avoids
re-injecting messages, but each vendor reuses its own conversation across requests.
Large aggregate cached-input counts reflect provider conversation reuse across
many requests, not dispatcher replay. The DB/router itself starts no model calls.
