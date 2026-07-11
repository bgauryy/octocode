# Compact Attend And Workboard Navigation

`attend` is the bounded lobby for a run. It reads live SQLite state and routes the
agent to one next action; it does not create another memory store.

```bash
octocode-awareness attend --workspace "$PWD" --query "current task" --compact
```

## Compact Contract

Compact `attend` is action-oriented and byte-budgeted. It includes:

- workspace identity and generated time;
- actionable counts/rows for Ready, Claimed, Verify, FilesUnderWork, and Inbox;
- at most one small relevant evidence item or warning;
- peer/task/run identifiers needed to drill down;
- omitted counts;
- one copy-runnable `next` command.

It omits clean projection detail, constant team norms, duplicate profile/organ/drive
aliases, repeated raw IDs, full bodies, and full file lists. Compact FilesUnderWork
rows keep path/peer_count/locked only — drill with `work list|show`. Noncompact attend
remains the explicit deep diagnostic surface.

Representative unit and CLI tests require compact attend to remain at or below 2 KB. Row count alone is
not sufficient; output-size assertions protect token cost. Workboard columns that are
empty are omitted; `counts` still reports totals for Ready/Claimed/Verify/FilesUnderWork/Inbox.

`--compact` minifies JSON and bounds agent-facing list defaults; explicit `--limit`,
`--full`, or `--include-bodies` restores deliberate depth. `attend`, memory recall,
and selected lists also reduce fields. `docs show` raw Markdown is the smaller
agent-readable form; its compact form is a JSON envelope.

For generic `query workboard --limit N`, the limit applies per lane, not to the whole
response; compact mode defaults to one row per lane. It can still exceed compact attend. Use `attend` for the next action,
targeted `verify audit`/`signal list`/`work show` for one concern, and CSV/HTML for
bulk review. Noncompact `attend` is a deliberate deep diagnostic, not a prompt-safe
default.

## Progressive Disclosure

| Need | Read |
|---|---|
| Start/resume | `attend --compact` |
| Shared task choices | `task ready|list|show` |
| Active file peers | `work list --compact`, then `work show --workspace "$PWD" --file <path>` |
| Operational counts | `workspace status --compact` |
| Verification debt | `verify audit --compact` |
| Reusable lessons | `memory recall --compact`; use `--explain --full` for score components |
| Inbox | `signal list --limit 3`; include bodies only when acting |
| Human cross-view inspection | `query all --format html` |

Compact workspace status returns exact `lock_count`, `lock_shown_count`, and
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

## Delta Delivery

Prompt/session briefings and peer notices use `delivery_state` fingerprints by
consumer, channel, and scope.

- First changed state: emit one bounded summary.
- Same state on next prompt/edit: emit nothing.
- Prompt memory: use the transient current prompt to select at most one scoped lead;
  require two meaningful token matches and emit nothing for unrelated memory.
- Hook briefing: at most five items and 1 KiB after UTF-8-safe truncation; drill into
  `signal list`, recall, or targeted queries for full data.
- Peer/signal/briefing changes: emit the new bounded state.
- Signal delivery does not mark read; `signal ack` is separate.

Pi also fingerprints unchanged verification sets so repeated agent-end events do not
repeat the same reminder. Pi captures the latest `input` text only in process until
`before_agent_start`; shell prompt hooks pass the same bounded query directly. Neither
path stores the prompt.

## Evidence Rules

Memory, peers, signals, and generated projections are leads. Check current files,
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
- Maintenance: pending runs, open signals, and missing memory file references older
  than one day by default (`--pressure-age-days` may raise the review window). These
  are read-only sensors with bounded IDs; they never delete,
  resolve, or verify fresh work.

Re-run attend after a material task, peer, signal, or verification transition—not
after every tool call.

Counts are workspace-wide; routing is actor-safe. For example, `Verify` may count
other agents' debt while `next` routes only verification owned by the current agent.
