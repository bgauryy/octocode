# Awareness Harness Invariants

Maintainer contract for the CLI, runtime library, host hooks, bundled skill, and
generated projections. Architecture narrative lives in [HOW_IT_WORKS.md](HOW_IT_WORKS.md).

## Canonical Boundaries

- Global SQLite is operational truth. `.octocode/` is a projection; plan folders
  contain narrative only.
- `schema commands` and JSON schemas own the public command contract.
- Canonical code lives in `src/**`, `bin/**`, and `scripts/schema.mjs`.
- Canonical skill guidance lives in `skills/octocode-awareness/**`.
- Build outputs and `.agents/skills/**` are regenerated, never hand-edited.

## Execution Invariants

1. A plan task has at most one leased claim/run.
2. A task claim or explicit `work start` is a reusable work-unit boundary; a host
   session is not.
3. Every structured write declares advisory `run_files` presence before editing.
4. Advisory peers can share a file. Exclusive acquisition rejects any other live
   presence; exclusive state blocks later presence.
5. Agent/session/task/plan identity is derived through `task_runs`, not copied into
   run-file or lock rows.
6. Task submit/release/expiry and verification update task, run, run files, locks,
   and audit events atomically.
7. TTL clears abandoned coordination only. Success requires `verify mark`.
8. Hook infrastructure failures warn/fail open except real exclusive conflicts,
   harness guard denial, and supported stop verification gates.

## Context Invariants

- Successful ordinary hooks are silent.
- Peer and briefing delivery is fingerprinted; unchanged content is not repeated.
- Bounded outputs include counts and `omitted_count`; full detail is opt-in.
- Compact attend has a byte-budget test and avoids repeated profile/organ/drive IDs.
- Signals remain unread until explicitly acknowledged; delivery dedupe is separate.
- Session handoffs are content-deduped.

## Host Parity

| Behavior | Shell hosts | Pi |
|---|---|---|
| Guard before presence | integrated pre-edit runner | tool-call guard |
| Advisory declaration | pre-edit | tool call/start |
| Edit audit/heartbeat | post-edit | tool result/end |
| Changed briefing | prompt/session start | before agent start |
| Verification gate | Stop/SubagentStop | bounded agent-end reminder |
| Handoff capture | SessionEnd/PreCompact | shutdown/pre-compact |

Claude may run skill frontmatter. Codex/Cursor require explicit installed config.
Pi never uses shell hook installation.

## Self-Improvement Boundary

```text
reflect -> mine weakness -> export proposal -> human/user approval
        -> source edit -> tests/review -> close feedback
```

`export-harness` and memories propose; they never patch instructions automatically.
Harness source edits require `OCTOCODE_ALLOW_HARNESS_APPLY=1` and a safe non-main
branch.

## Verification Matrix

```bash
yarn workspace @octocodeai/octocode-awareness typecheck
yarn workspace @octocodeai/octocode-awareness test:quiet
yarn workspace @octocodeai/octocode-awareness build
yarn workspace @octocodeai/octocode-awareness test:smoke
node skills/octocode-skills/scripts/skill-review.mjs \
  packages/octocode-awareness/skills/octocode-awareness
```

Migration tests must cover v1 execution tables, v2 `files_json`/typed locks, and v3
normalized run files/exclusive locks. Hook tests must replay equivalent shell/Pi
events. Output tests must enforce byte/detail caps, not only row counts.
