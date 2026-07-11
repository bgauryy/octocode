# Data Model Relationships

| Owner | Dependents |
|---|---|
| `plans` | `plan_members`, `plan_docs`, `tasks` |
| `tasks` | `task_paths`, `task_dependencies`, `task_claims`, `task_events`, `task_runs` |
| `task_runs` | `run_files`, `locks`, `run_log`, `edit_log`, `harness_log` |
| agents | sessions, plans, claims, runs, signals, memories |
| `signals` / `memories` | `signal_reads` / `memory_refs` |

## Readiness

Ready task: `OPEN`, no live claim, and no dependency whose status is not `DONE`.

## File State

Active file work:

```sql
SELECT rf.file_path, r.run_id, r.agent_id, r.task_id, r.rationale
FROM run_files rf JOIN task_runs r ON r.run_id = rf.run_id
WHERE rf.ended_at IS NULL AND rf.expires_at > ? AND r.status = 'ACTIVE';
```

Exclusive state is `EXISTS locks(run_id,file_path)`. Advisory start rejects another
run's lock; exclusive acquisition rejects another run's active file presence.

Reason display is `reason_override`, else task/run reasoning. Plan/task/agent/session
are always joined through `task_runs`, never copied into `run_files` or `locks`.

## Completion And Delivery

Verification debt is `task_runs.status='PENDING'`; linked task completion follows
`verify mark`. `delivery_state` fingerprints output only; signal read state remains in
`signal_reads`.
