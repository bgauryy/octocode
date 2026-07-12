# Awareness Harness Invariants

Maintainer contract for the CLI, runtime library, host hooks, bundled skill, and
generated projections. Architecture narrative lives in [HOW_IT_WORKS.md](HOW_IT_WORKS.md).

## Canonical Boundaries

- Global SQLite is operational truth. `.octocode/` is a projection; plan folders
  contain narrative only.
- `schema commands` and JSON schemas own the public command contract.
- Canonical code and Zod contracts live in `src/**` and `bin/**`.
- Canonical skill guidance lives in repo-root `skills/octocode-awareness/**`.
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

## Homeostatic And Token Invariants

- Token pressure is regulated: stable state stays silent; changed state emits only
  the next decision packet; detail remains queryable outside the prompt.
- Every control action has a sensor, target, actuator, and guard. A recommendation
  without re-measurement is an open loop, not improvement.
- Prompt hooks may preview maintenance pressure but never archive, prune, rebuild,
  or rewrite state. Applying maintenance is an explicit reviewed command.
- Reflection, memory, generated wiki, and transactive maps are diagnostic leads.
  They cannot override current instructions, source, tests, or human authority.
- The living-system language is an operational metaphor, never a claim of
  sentience, autonomy, self-selected goals, or cross-machine synchronization.

## Host Parity

| Behavior | Shell hosts | Pi |
|---|---|---|
| Guard before presence | integrated pre-edit runner | tool-call guard |
| Advisory declaration | pre-edit | tool call/start |
| Edit audit/heartbeat | post-edit | tool result/end |
| Changed briefing | prompt/session start | before agent start |
| Verification gate | Stop/SubagentStop | bounded agent-end reminder |
| Pre-compact finalize/capture | PreCompact (Codex, Cursor only) | pre-compact; session remains reusable |
| Session-end finalize/capture | SessionEnd (Claude, Cursor only) | shutdown; session is ended |

Claude may run skill frontmatter; do not also install duplicate project settings.
Codex/Cursor require explicit installed config.
Pi never uses shell hook installation. Claude installs `SessionEnd` but not
`PreCompact`; Codex installs `PreCompact` but not `SessionEnd` (unsupported);
Cursor installs both. See `docs/HOOKS.md` Host Support.

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
```

Migration tests must cover legacy generation-1 execution tables, generation-2
`files_json`/typed locks, generation-3 normalized run files/exclusive locks, and
the canonical OCT1/v1 identity. Hook tests must replay equivalent shell/Pi
events. Output tests must enforce byte/detail caps, not only row counts.
