# Plan, Task, And Standalone WORK

One durable queue exists: plan `tasks`. Never create “today's tasks” in Markdown,
memory, or refinements. Inspect attend/Ready/Claimed/Verify; claim a matching task or open explicit Work with reason, files, and test plan.

## Lead: create shared work

```bash
<cli> plan create --name "Release" --objective "Ship safely" \
  --lead-agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --compact
# author docs/DESIGN.md inside the returned plan folder, then register it
<cli> plan doc --plan-id plan_123 --agent-id "$OCTOCODE_AGENT_ID" \
  --path docs/DESIGN.md --title "Design" --compact
<cli> task create --plan-id plan_123 --agent-id "$OCTOCODE_AGENT_ID" \
  --title "Implement parser" --reasoning "Needed before integration" \
  --acceptance "parser tests pass" --path src/parser.ts --compact
```

Tasks require reasoning, acceptance, and 1+ paths; `--depends-on task_...` orders them.
New plans are ACTIVE. PAUSED retains work but blocks claims. Complete/cancel only after active runs resolve. SQLite owns task state; plan prose lives under
`.octocode/plan/<timestamp-name>/` under the exact `--workspace` you pass (repo root when omitted; the plan row always scopes to the repo root for discovery) and never duplicates a mutable checklist.

## Agent: execute plan task

```bash
<cli> task claim --task-id task_123 --agent-id "$OCTOCODE_AGENT_ID" --compact
# hooks declare files; without hooks, attach them to the returned run
<cli> work start --run-id run_123 --agent-id "$OCTOCODE_AGENT_ID" --file src/a.ts --compact
# run the acceptance check
<cli> task submit --task-id task_123 --run-id run_123 \
  --agent-id "$OCTOCODE_AGENT_ID" --message "ready for verification" --compact
<cli> verify mark --run-id run_123 --agent-id "$OCTOCODE_AGENT_ID" \
  --message "tests pass" --compact
```

Heartbeat long claims with `task heartbeat --task-id <task> --run-id <run>
--agent-id "$OCTOCODE_AGENT_ID" --compact`. `task release` returns unfinished work to OPEN/BLOCKED.
Dependencies and ACTIVE plan status derive readiness; never set READY manually.
## Standalone WORK

```bash
<cli> work start --agent-id "$OCTOCODE_AGENT_ID" --file README.md \
  --rationale "small docs fix" --test-plan "review diff" --compact
# run the declared check
<cli> work end --run-id run_123 --agent-id "$OCTOCODE_AGENT_ID" --compact
<cli> verify mark --run-id run_123 --agent-id "$OCTOCODE_AGENT_ID" \
  --message "reviewed" --compact
```
Add `--exclusive` only for sensitive work. A new explicit start creates a new run; only explicit `--run-id` or a host hook extends one.
