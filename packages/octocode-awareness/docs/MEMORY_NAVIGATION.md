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

`--compact` is not a universal field-reduction promise. It minifies JSON for every
command, while documented lean surfaces such as `attend`, memory recall, and selected
lists also reduce fields or summarize bodies. `docs show` raw Markdown is the smaller
agent-readable form; its compact form is a JSON envelope.

For generic `query workboard --limit N`, the limit applies per lane, not to the whole
response. It can be much larger than compact attend. Use `attend` for the next action,
targeted `verify audit`/`signal list`/`work show` for one concern, and CSV/HTML for
bulk review. Noncompact `attend` is a deliberate deep diagnostic, not a prompt-safe
default.

## Progressive Disclosure

| Need | Read |
|---|---|
| Start/resume | `attend --compact` |
| Shared task choices | `task ready|list|show` |
| Active file peers | `work list --compact`, then `work show --file <path>` |
| Operational counts | `workspace status --compact` |
| Verification debt | `verify audit --compact` |
| Reusable lessons | `memory recall --compact`; request full/explain only when judging ranking |
| Inbox | `signal list --limit 5`; include bodies only when acting |
| Human cross-view inspection | `query all --format html` |

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
- Peer/signal/briefing changes: emit the new bounded state.
- Signal delivery does not mark read; `signal ack` is separate.

Pi also fingerprints unchanged verification sets so repeated agent-end events do not
repeat the same reminder.

## Evidence Rules

Memory, peers, signals, and generated projections are leads. Check current files,
tests, and user instructions before acting. Zero recall results mean broaden one
query/filter; they do not prove absence.

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

Re-run attend after a material task, peer, signal, or verification transition—not
after every tool call.

Counts are workspace-wide; routing is actor-safe. For example, `Verify` may count
other agents' debt while `next` routes only verification owned by the current agent.
