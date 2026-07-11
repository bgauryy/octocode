# File Work, Exclusive Locks, And Verification

File awareness and file exclusion are different operations:

- `work *` is mandatory advisory presence. Multiple agents may share a file.
- `lock *` is optional exclusive protection for sensitive work.
- verification proves the promised check; ending presence or expiring a lock does not.

## Ordinary Work

```bash
octocode-awareness work start --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --file src/auth.ts \
  --rationale "refactor token refresh" --test-plan "yarn test auth" --compact
```

This creates an explicit `origin=WORK` run when `--run-id` is absent, then upserts
`run_files`. Add files to the same explicit run:

```bash
octocode-awareness work start --agent-id "$OCTOCODE_AGENT_ID" \
  --run-id run_abc --file src/session.ts --compact
octocode-awareness work touch --agent-id "$OCTOCODE_AGENT_ID" \
  --run-id run_abc --compact
```

Task-backed callers pass the run returned by `task claim`. Hooks do this automatically
when exactly one live task claim applies.

Ordinary overlap succeeds. `work start`/pre-edit returns bounded peer changes with
agent, task/run, short reason, and exclusive state. Use full detail only when needed:

```bash
octocode-awareness work show --workspace "$PWD" --file src/auth.ts --compact
octocode-awareness work list --workspace "$PWD" --compact
```

## Sensitive Exclusive Work

Open explicit work with `--exclusive`:

```bash
octocode-awareness work start --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --file migrations/001.sql \
  --rationale "change account schema" --test-plan "yarn test migrations" \
  --exclusive --compact
```

Or upgrade a known task/run with `lock acquire`. Locks are exclusive-only; SHARED
locks no longer exist.

Conflict law:

| Request | Other state | Result |
|---|---|---|
| Advisory presence | Advisory presence | Allowed; peers shown |
| Advisory presence | Exclusive lock | Blocked before presence is created |
| Exclusive lock | Other live presence | Blocked; coordinate first |
| Exclusive lock | Same run presence | Allowed/renewed |

Exit `2` means a real conflict or bounded wait timeout. Read the holder/reason, then
signal, wait, switch work, or prune only after expiry. Never steal live exclusivity.

```bash
octocode-awareness lock wait --agent-id "$OCTOCODE_AGENT_ID" \
  --target-file migrations/001.sql --wait-seconds 120 --compact
```

Wait observes only; acquire after a clear result.

## Ending Work

Explicit work:

```bash
# run the declared check
octocode-awareness work end --agent-id "$OCTOCODE_AGENT_ID" \
  --run-id run_abc --compact
octocode-awareness verify mark --agent-id "$OCTOCODE_AGENT_ID" \
  --run-id run_abc --message "auth tests passed" --compact
```

`work end` closes run-file presence, releases its locks, and moves a completed
standalone WORK run to `PENDING`. A TASK run must use `task submit` or `task release`.

```bash
# run the acceptance check while presence remains active
octocode-awareness task submit --task-id task_abc --run-id run_abc \
  --agent-id "$OCTOCODE_AGENT_ID" --message "ready for verification" --compact
octocode-awareness verify mark --run-id run_abc \
  --agent-id "$OCTOCODE_AGENT_ID" --message "acceptance checks passed" --compact
```

Successful verification moves the linked task to `DONE`; failure moves it to
`FAILED`. `verify audit` is the final debt-listing gate. If deliberately using
`verify mark --all-pending`, scope it by workspace.

## Automatic Hook Fallback

If no task claim or explicit WORK presence matches a structured write, pre-edit
creates or reuses one scoped `origin=HOOK` aggregate. Post-edit records and touches
it; Stop, PreCompact, or SessionEnd finalizes it once to `PENDING`. PreCompact keeps
the session reusable; SessionEnd marks it ended. Aggregates never cross agent,
session, workspace, artifact, TASK, or explicit WORK boundaries.

## TTL And Cleanup

Presence and lock TTL recover from crashes. Heartbeat extends active work. Expiry:

- makes stale presence inactive;
- removes stale exclusive protection;
- never marks work successful;
- never moves a live TASK claim to `PENDING`.

Task claim lease is separate and refreshed with `task heartbeat`. Claim expiry closes
the run's presence/locks, fails that attempt, and returns the task to `OPEN`.

Preview cleanup:

```bash
octocode-awareness lock prune --workspace "$PWD" --expired-only --dry-run --compact
```

## Path Coverage

Write-tool hooks declare recognized paths before editing. External processes and
arbitrary shell side effects may not be observable in real time; session/dirty-tree
reconciliation reports undeclared files. Without active hooks, agents must call
`work start|touch` explicitly.
