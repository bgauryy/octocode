# Plan, Task, And Standalone WORK

There is one durable queue: `tasks` under a plan. Never create “today's tasks” in
Markdown, memory, or refinements.

## Choose

1. Inspect attend/workboard Ready, Claimed, Verify, and `task ready|list`.
2. Claim a matching ready task; its leased run is the work-unit boundary.
3. If no task fits, open explicit `work start` with reason, files, and test plan.
4. Create plan/tasks only when authorized; plan lead governs lifecycle/docs.

## Plan Task

```bash
<cli> task claim --task-id task_123 --agent-id "$OCTOCODE_AGENT_ID" --compact
# hooks declare edited files; without hooks:
<cli> work start --run-id run_123 --agent-id "$OCTOCODE_AGENT_ID" --file src/a.ts --compact
<cli> task submit --task-id task_123 --run-id run_123 --agent-id "$OCTOCODE_AGENT_ID" --compact
# run acceptance check
<cli> verify mark --run-id run_123 --agent-id "$OCTOCODE_AGENT_ID" --message "passed" --compact
```

Heartbeat long claims. `task release` returns unfinished work to OPEN/BLOCKED.
Dependencies derive readiness; agents never set READY manually.

## Standalone WORK

```bash
<cli> work start --agent-id "$OCTOCODE_AGENT_ID" --file README.md \
  --rationale "small docs fix" --test-plan "review diff" --compact
# edit
<cli> work end --run-id run_123 --agent-id "$OCTOCODE_AGENT_ID" --compact
<cli> verify mark --run-id run_123 --agent-id "$OCTOCODE_AGENT_ID" --message "reviewed" --compact
```

Add `--exclusive` only for sensitive work. Never infer one quick run from a host
session; explicit start or task claim defines reuse.

Plan docs live under `.octocode/plan/<timestamp-name>/`; SQLite owns live task state.
