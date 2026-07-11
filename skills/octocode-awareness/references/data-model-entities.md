# Data Model Entities

`plans` stores objective, lead, status, scope, and plan folder. Members participate;
docs register `PLAN.md` and supporting narrative.

`tasks` stores durable work: required reasoning/acceptance, planning paths, priority,
status, and dependency graph. `task_claims` leases one agent/run. Readiness is
derived; there is no READY row or second task list.

`task_runs` stores one attempt with origin `TASK|WORK|HOOK`, agent/session, rationale,
test plan, scope, and `ACTIVE|PENDING|SUCCESS|FAILED` status.

`run_files` stores `(run_id,file_path)`, optional reason override, source, heartbeat,
expiry, and end time. It is mandatory advisory “under work” state. `locks` stores
only exclusive `(run,path)` protection. `edit_log` is completed edit history.

```text
task: OPEN -> IN_PROGRESS -> VERIFY -> DONE|FAILED
                    \-> OPEN|BLOCKED
run:  ACTIVE -> PENDING -> SUCCESS|FAILED
```

`delivery_state` suppresses unchanged prompt delivery without acknowledging signals.
Memories store reusable learning; signals store peer threads; refinements store owned
follow-up; sessions group host activity. Agent identity is cooperative, not security.
