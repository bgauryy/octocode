# Exclusive Lock And Verification Protocol

Read `plan-task-workflow.md` first. Ordinary writes use advisory work, not locks.

## Exclusive

Use `work start --exclusive` or task/run-aware `lock acquire` only when concurrent
editing would be unsafe. Locks are exclusive-only.

- Existing other presence -> acquisition exits 2; coordinate/wait.
- Existing exclusive -> advisory start exits 2 before presence.
- Same run -> acquire/renew allowed.
- TTL -> crash recovery only, never success.

## Wait Vs Presence

`lock wait` observes **other agents' lock rows only**. It does **not** mean peers are
gone: advisory `run_files` presence can still block exclusive acquire (`ACTIVE_WORK`).
After wait returns clear, re-check `work show --workspace "$PWD" --file <path>` before acquire. Expiry
cleans coordination state; it never proves completion. `verify audit` lists
`stale_active` when a run is ACTIVE with no live presence.

`lock prune --expired-only --dry-run` previews abandoned protection cleanup. Do not
steal live exclusivity.

## Close And Verify

Explicit WORK:

```bash
# run declared check
<cli> work end --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --compact
<cli> verify mark --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --message "passed" --compact
```

TASK work uses `task submit`/`task release`; terminal `work end` is rejected. Successful
`verify mark` closes the run and linked task. Failure closes them as failed.

Post-edit logs/heartbeats and keeps the scoped HOOK aggregate ACTIVE. Stop,
PreCompact, or SessionEnd finalizes it once to PENDING. PreCompact keeps the session
reusable; SessionEnd marks it ended. Stop output caps debt; Pi may remind instead of
block. `verify audit` lists debt (exit **1** when debt remains).
If deliberately using `verify mark --all-pending`, scope it by workspace.
For proven abandonment, mark the exact run `FAILED`; `verify audit` remains read-only.

Presence/lock expiry never moves a live TASK run to PENDING. Task claim expiry is a
separate atomic lifecycle that fails its attempt and returns the task to OPEN.

Exit codes: **2** = lock conflict or wait timeout; **1** = verify debt / validation.
