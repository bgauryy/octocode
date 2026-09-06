# Awareness harness invariants

Maintainer contract for the CLI, runtime library, host hooks, bundled skill, and
read-only query exports. Architecture narrative lives in [HOW_IT_WORKS.md](HOW_IT_WORKS.md).

## Canonical boundaries

- The selected Awareness SQLite database is operational truth. Global scope
  defaults to `$OCTOCODE_HOME/awareness/awareness.sqlite3`; repository scope uses
  `.octocode/awareness.sqlite3` only when explicitly selected. Adjacent exports
  and plan folders aren't live state.
- Agent databases and worker/runtime ledgers are outside Awareness ownership.
- `schema commands` and JSON schemas own the public command contract.
- Canonical code and Zod contracts live in `src/**` and `bin/**`.
- Canonical skill guidance lives in package-local `skills/octocode-awareness/**`.
- Build outputs and `.agents/skills/**` are regenerated, never hand-edited.

## Execution invariants

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

## Context invariants

- Successful ordinary hooks are silent.
- Peer and briefing delivery is fingerprinted; unchanged content is not repeated.
- Bounded outputs include counts and `omitted_count`; full detail is opt-in.
- Compact attend has a byte-budget test and avoids repeated profile/organ/drive IDs.
- Signals remain unread until explicitly acknowledged; delivery dedupe is separate.
- Session handoffs are content-deduped.

## Homeostatic and token invariants

- Token pressure is regulated: stable state stays silent; changed state emits only
  the next decision packet; detail remains queryable outside the prompt.
- Every control action has a sensor, target, actuator, and guard. A recommendation
  without re-measurement is an open loop, not improvement.
- Prompt hooks may preview maintenance pressure but never archive, prune, rebuild,
  or rewrite state. Applying maintenance is an explicit reviewed command.
- Reflection, memory, and transactive maps are diagnostic leads.
  They cannot override current instructions, source, tests, or human authority.
- The living-system language is an operational metaphor, never a claim of
  sentience, autonomy, self-selected goals, or cross-machine synchronization.

## Host parity

Host adapters translate their native lifecycle into the shared runner and coordination
store. They must preserve the same guard-before-presence, edit receipt, changed-state
briefing, verification, compaction, and session-end invariants where the host exposes an
equivalent event.

[`HOOKS.md`](HOOKS.md) is the sole host support matrix. It owns exact event names,
installation surfaces, platform limitations, runtime-health semantics, and the Pi
in-process exception. Do not duplicate that matrix here.

## Self-improvement boundary

```text
reflect -> mine weakness -> export proposal -> human/user approval
        -> source edit -> tests/review -> close feedback
```

`export-harness` and memories propose; they never patch instructions automatically.
Harness source edits require `OCTOCODE_ALLOW_HARNESS_APPLY=1` and a safe non-main
branch.

## Verification matrix

```bash
yarn workspace @octocodeai/octocode-awareness typecheck
yarn workspace @octocodeai/octocode-awareness test:quiet
yarn workspace @octocodeai/octocode-awareness build
yarn workspace @octocodeai/octocode-awareness test:smoke
```

Migration tests must cover legacy execution tables, normalized run files and
exclusive locks, mixed Agent/Awareness stores, and the distinct current
Awareness identity. Hook tests must replay equivalent shell/in-process events.
Output tests must enforce byte/detail caps, not only row counts.
