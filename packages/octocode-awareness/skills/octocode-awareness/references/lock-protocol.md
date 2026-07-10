# Exclusive Lock And Verification Protocol

Read `plan-task-workflow.md` first. Ordinary writes use advisory work, not locks.

## Exclusive

Use `work start --exclusive` or task/run-aware `lock acquire` only when concurrent
editing would be unsafe. Locks are exclusive-only.

- Existing other presence -> acquisition exits 2; coordinate/wait.
- Existing exclusive -> advisory start exits 2 before presence.
- Same run -> acquire/renew allowed.
- TTL -> crash recovery only, never success.

`lock wait` observes without claiming. `lock prune --expired-only --dry-run` previews
abandoned protection cleanup. Do not steal live exclusivity.

## Close And Verify

Explicit WORK:

```bash
<cli> work end --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --compact
# run declared check
<cli> verify mark --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --message "passed" --compact
```

TASK work uses `task submit`/`task release`; terminal `work end` is rejected. Successful
`verify mark` closes the run and linked task. Failure closes them as failed.

Automatic HOOK fallback becomes PENDING after post-edit. Stop output caps debt; Pi
may remind instead of block. `verify audit` lists debt. If deliberately using
`verify mark --all-pending`, scope it by workspace. `verify audit --abandon` is only
for real abandonment.

Presence/lock expiry never moves a live TASK run to PENDING. Task claim expiry is a
separate atomic lifecycle that fails its attempt and returns the task to OPEN.
