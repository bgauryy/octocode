# Compact Attend And Workboard Navigation

`attend` reads a bounded page of registered workspace peers by default. Attend once
per workspace/session, or reuse the host briefing. `last_seen_at` records activity;
it does not prove a process is live. Message delivery belongs to host lifecycle
hooks, not repeated attendance.

```bash
npx @octocodeai/octocode-awareness attend --workspace "$PWD" --compact
```

Presence pages expose `partial`, `offset`, and an executable `next.list.command`
when more peers exist. Default limit is 10; the supported range is 1–50. API
continuations are request objects; CLI continuations retain arguments. Follow them
with the same store and workspace context. See the [API reference](API.md).

## Detailed observer

Use `--details` when work, memory, or verification state can change the next action.
Task queries, file/scope filters, revisions, or explicit body requests also select
this observer. Presence `offset` pagination is unavailable in detailed mode.

Detailed compact `attend` is action-oriented and byte-budgeted. It includes:

- workspace identity and generated time;
- actionable counts/rows for Ready, Claimed, Verify, FilesUnderWork, and Inbox;
- at most one small relevant evidence item or warning;
- peer/task/run identifiers needed to drill down;
- omitted counts;
- one structured `next` action and, when safe and available, its literal command name and arguments.

It omits clean projection detail, constant team norms, duplicate profile/organ/drive
aliases, repeated raw IDs, full bodies, and full file lists. Compact FilesUnderWork
rows keep path/peer_count/locked only — drill with `work list|show`. Noncompact detailed attend
remains the explicit deep diagnostic surface.

Representative unit and CLI tests require detailed compact attend to remain at or below 2 KB. Row count alone is
not sufficient; output-size assertions protect token cost. Workboard columns that are
empty are omitted; `counts` still reports totals for Ready/Claimed/Verify/FilesUnderWork/Inbox.

`--compact` minifies JSON and bounds agent-facing list defaults; explicit `--limit`,
`--full`, or `--include-bodies` restores deliberate depth. `attend`, memory recall,
and selected lists also reduce fields. `docs show` raw Markdown is the smaller
agent-readable form; its compact form is a JSON envelope.

For generic `query workboard --limit N`, the limit applies per lane, not to the whole
response; compact mode defaults to one row per lane. It can still exceed compact attend. Use detailed `attend` for the next action,
targeted `verify audit`/`signal list`/`work show` for one concern, and CSV/HTML for
bulk review. Noncompact `attend --details` is a deliberate deep diagnostic, not a prompt-safe
default.

## Progressive Disclosure

| Need | Read |
|---|---|
| Start/resume | Reuse the host briefing or `attend --compact` once |
| Work/evidence routing | `attend --details --compact` |
| Shared task choices | `task ready|list|show` |
| Active file peers | `work list --compact`, then `work show --workspace "$PWD" --file <path>` |
| Operational counts | `status --compact` |
| Verification debt | `verify audit --compact` |
| Reusable lessons | `memory recall --compact`; use `--explain --full` for score components |
| Inbox | `signal list --limit 3`; include bodies only when acting |
| Human cross-view inspection | `query all --format html` |

Compact status returns exact `lock_count`, `lock_shown_count`, and
`lock_omitted_count`, plus at most one lean lock lead. Drop `--compact` and set
`--limit` only when full lock rows are needed.

Compact `work start|touch|end` likewise returns exact file/peer totals and at most
one lean lead for each. Use non-compact output or targeted `work show --workspace "$PWD" --file` when
full presence records are needed.

`query workboard` groups active work by relative path. Each FilesUnderWork row caps
peers at three, includes task/plan/reason and exclusive state, and reports
`omitted_peer_count` instead of dumping all agents. Workboard lane truncation uses
`omitted_count` separately; there is no cursor pagination, so drill into a targeted
surface instead of repeatedly increasing the lane limit.

## Scoped attend revisions

A detailed `attend` response includes an opaque `revision` and `unchanged: false`.
Repeat the same actor, store, workspace and filters with `--revision <returned-token>`.
An unchanged response retains the pending `next` action and unavailable sensors;
retain the previous full packet. Fresh queries still execute, so this saves output
context, not database work. Invisible memory-score drift does not invalidate the
revision; changed selected evidence, expiry, blockers and debt do.

Tokens bind protocol/schema version and the physical store. Invalid, foreign,
partial or concurrently changing observations return a full fallback with a reset
reason. A revision is not a lease, cached permission or proof of a complete database
snapshot. Read current admission state before mutations. See
[`attend-revision.ts`](../src/attend-revision.ts) and its
[regressions](../tests/attend-revision.test.ts).

## Delta delivery

Peer notices use `delivery_state` fingerprints by consumer, channel, and scope.
Changed unread messages produce a bounded packet; unchanged messages stay silent.
Sender, message, and thread IDs are preserved, and clipped bodies expose partial
state plus executable retrieval. Hook communication does not query memory,
refinements, or verification. Unsupported response channels do not consume the
fingerprint needed by a later supported boundary.

Delivery is event-driven. Pi drains at session start and agent completion after
session persistence is available, and a native database/WAL watcher schedules an
authoritative drain when an idle message arrives. Watch/read failures retry with
bounded fast delays followed by a sparse 30-second recovery interval; lifecycle
drains remain the fallback. Native delivery receipts, `signal ack` for handling,
and `signal resolve` for completion are separate. See
[peer event delivery](HOW_IT_WORKS.md#peer-event-delivery).

## Evidence Rules

Memory, peers, signals, and query exports are leads. Check current files,
tests, and user instructions before acting. Zero recall results mean broaden one
query/filter; they do not prove absence.

`memory recall --smart` first uses requested filters, then widens an under-filled
result by dropping label/tag/minimum-importance restrictions. Output reports
`smart_expanded` and `smart_dropped_filters`; widening is never silent. File-backed
attend evidence is an `existing_file_lead` when the path exists and `needs_refs` when
missing; neither label proves the cited content. Explicit recall updates popularity,
not evidence recency. Lean rows cap tags/references and omit absent optional fields;
use `--full` only for the selected row.

Use file/scope filters before increasing limits. Prefer relative paths in compact
output. Use HTML/CSV or explicit full rows for bulk inspection rather than raising
the prompt budget.

## Workboard Ownership

The workboard is derived; it has no table. Lanes route actions:

- Ready: claim a dependency-ready task.
- Claimed: heartbeat/continue/coordinate.
- FilesUnderWork: inspect overlaps or exclusivity.
- Verify: run declared checks and mark results.
- Inbox: act, acknowledge, resolve.
- MemoryReview/DeveloperReview/ProjectionHealth: bookkeep or housekeep.
- Maintenance: pending runs, stale ACTIVE runs, open signals, and missing memory
  file references older than one day by default (`--pressure-age-days` may raise
  the review window). Workboard rows are bounded sensors. `maintenance digest
  --dry-run` previews recovery; applying digest may resolve stale handoff
  broadcasts and mark expired ACTIVE runs `FAILED` with an audit receipt, but it
  never marks work successful from age.

Re-run detailed attend only when a material task, peer, signal, or verification
transition needs broader inspection. A targeted read is enough for a known concern.

Counts are workspace-wide; routing is actor-safe. For example, `Verify` may count
other agents' debt while `next` routes only verification owned by the current agent.
