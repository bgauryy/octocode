# Awareness Database

Canonical store: `~/.octocode/memory/awareness.sqlite3`, or
`$OCTOCODE_MEMORY_HOME/awareness.sqlite3`. Foreign keys are always enabled.
The runtime requires Node.js 22.13.0 or newer (`node:sqlite` without a flag).
Awareness uses WAL only when the embedded SQLite contains the concurrent reset-race
fix (3.44.6, 3.50.7, 3.51.3, or a newer fixed release); affected versions use
rollback journaling instead. Current production schema version is 3. Source:
`src/db.ts`; the isolated final-schema bootstrap is under `src/v4/`.

`<workspace>/.octocode/` is not the database. It contains generated projections and
authored plan documents.

The runtime/version rationale and primary SQLite/Node sources are mapped in
[REFERENCES.md](REFERENCES.md).

## Collaboration Graph

```text
plans -> plan_members / plan_docs / tasks
tasks -> task_paths / task_dependencies / task_claims / task_events / task_runs
task_runs -> run_files / locks / run_log / edit_log / harness_log
agents -> sessions / plans / claims / runs / memories / signals
```

```mermaid
erDiagram
  plans ||--o{ tasks : contains
  plans ||--o{ plan_members : has
  plans ||--o{ plan_docs : documents
  tasks ||--o{ task_paths : plans
  tasks ||--o{ task_dependencies : waits_for
  tasks ||--o| task_claims : leased_by
  tasks ||--o{ task_runs : attempted_as
  task_runs ||--o{ run_files : declares
  task_runs ||--o{ locks : protects
  task_runs ||--o{ run_log : verifies
  task_runs ||--o{ edit_log : audits
  signals ||--o{ signal_reads : acknowledged_by
  memories ||--o{ memory_refs : cites
```

## Plans And Tasks

`plans` stores name, objective, lead, status, workspace/artifact scope, managed
`doc_dir`, and timestamps. Status is `DRAFT|ACTIVE|PAUSED|COMPLETED|CANCELLED`.
The lead is also the first `plan_members` row. `plan_docs` registers `PLAN.md` and
supporting files inside `.octocode/plan/<timestamp-name>/`.

`tasks` stores the single durable work queue: plan, title, required reasoning,
acceptance criteria, priority, creator, timestamps, and status
`OPEN|IN_PROGRESS|BLOCKED|VERIFY|DONE|FAILED|CANCELLED`.

`task_paths` is non-exclusive planning scope. `task_dependencies` is an acyclic
same-plan graph. Readiness is derived: `OPEN`, no live claim, and every dependency
`DONE`. There is no `READY` status or “today's tasks” table.

`task_claims` leases one run/agent per task under `BEGIN IMMEDIATE`. Claim heartbeat
and expiry are independent from file presence. Expiry fails the abandoned run,
returns the task to `OPEN`, closes its file work/locks, and emits an event.

## Runs

`task_runs` is one attempt:

| Column | Meaning |
|---|---|
| `run_id` | Stable `run_...` identifier. |
| `task_id` | Nullable durable task; null for standalone WORK or hook fallback. |
| `origin` | `TASK`, explicit `WORK`, or automatic `HOOK`. |
| `agent_id`, `session_id` | Actor and optional host session. |
| `rationale`, `test_plan`, `context_ref` | Attempt intent and verification. |
| `status` | `ACTIVE`, `PENDING`, `SUCCESS`, or `FAILED`. |
| `workspace_path`, `artifact` | Operational scope. |
| timestamps | Creation/update. |

Task reasoning/test criteria are copied into a run as an attempt snapshot. A host
session never defines a reusable work unit; only task claim or explicit `work start`
does.

```text
TASK: OPEN -> IN_PROGRESS -> VERIFY -> DONE|FAILED
                       \-> OPEN|BLOCKED on release
RUN:  ACTIVE -> PENDING -> SUCCESS|FAILED
```

## Run Files: Mandatory Advisory Presence

`run_files` replaces `task_runs.files_json`:

| Column | Meaning |
|---|---|
| `(run_id,file_path)` | Primary key; normalized absolute path. |
| `reason_override` | Optional file-specific reason; otherwise use run/task reason. |
| `source` | `EXPLICIT` or `HOOK`. |
| `started_at`, `heartbeat_at`, `expires_at` | Presence lifecycle. |
| `ended_at` | Null while active. |

Active presence requires `ended_at IS NULL`, unexpired `expires_at`, and an ACTIVE
run. Multiple runs may be active on one ordinary path. Display context is derived:

```text
run_files -> task_runs(agent,session,reason) -> tasks -> plans
```

Do not copy agent/task/plan/general reason into `run_files`.

## Exclusive Locks

`locks(lock_id,file_path,run_id,acquired_at,expires_at)` contains only exclusive
protection. Agent/session come from `task_runs`; no lock type column exists.

- Advisory start conflicts only with another run's active lock.
- Exclusive acquisition conflicts with any other active run-file presence.
- A run may upgrade its own presence.
- TTL removes stale protection; it does not change TASK/WORK success.

See [LOCKS.md](LOCKS.md) for command behavior.

## Delivery State

`delivery_state(consumer_id,channel,scope_key,fingerprint,delivered_at)` suppresses
unchanged hook briefings and peer-state messages. It is delivery bookkeeping, not
signal acknowledgement: only `signal ack` writes `signal_reads`.

## Knowledge, Communication, Audit

| Table | Purpose |
|---|---|
| `agents`, `sessions` | Stable identity, scope, contiguous host activity. |
| `memories`, `memories_fts`, `memory_refs` | Durable lessons, lexical search, provenance, validity/supersession. |
| `signals`, `signal_reads` | Typed peer threads and explicit acknowledgement. |
| `refinements` | Owned follow-up/handoff/instruction feedback; not a task queue. |
| `run_log`, `task_events` | Verification and planning lifecycle history. |
| `edit_log` | Completed file edit audit. |
| `harness_log` | Reflection/harness lifecycle audit. |

JSON file lists remain on signals/refinements because they are message snapshots,
not normalized active run state.

## Scope

Use one normalized `workspace_path` across attend, plans, tasks, work, locks,
verification, signals, and handoff. `artifact` optionally narrows a workspace;
memory/signals/refinements may also use repo/ref.

Task paths are workspace-relative planning scope. Run-file and lock paths are
normalized absolute operational paths.

## Query Views

| View | Content |
|---|---|
| `plans`, `tasks`, `runs` | Planning and attempt lifecycle. |
| `work list|show` | Flat active run-file rows for focused CLI inspection. |
| FilesUnderWork | Workboard paths grouped with peers, reason, task/plan, and exclusive state. |
| `locks` | Live exclusive rows joined to run identity. |
| `workboard` | Inbox, Verify, Ready, Claimed, FilesUnderWork, RecentDone, review/health lanes. |
| `files`, `activity` | Historical references and edit/audit views. |

Compact views cap detail and expose omitted counts. Full rows are explicit.

## Migration

### v1 -> v2

Legacy execution `tasks` became `task_runs`; `task_log` became `run_log`; related
foreign keys were renamed. Migrated attempts were standalone.

### v2 -> v3

- Add `origin` and infer `TASK` when `task_id` exists, otherwise `HOOK`.
- Backfill `run_files` from `files_json` and lock rows.
- Rebuild `task_runs` without `files_json`.
- Rebuild locks as exclusive-only without duplicated agent/session/type fields.
- Add `delivery_state`.
- Set `PRAGMA user_version = 3`.

The complete migration runs under one `BEGIN IMMEDIATE` transaction. Concurrent first
openers wait for the winner, then observe schema v3; detection, DDL, indexes, and
`user_version` cannot interleave. The migration is idempotent and preserves run IDs
and audit history. Old clients that require v2 columns are not supported after
migration; rebuild/reinstall the bundled CLI/hooks together.

## Operations

- `maintenance init`: create/migrate.
- `workspace status`: operational counts and live state; compact output reports
  exact lock totals/omissions and only one lean lock lead.
- `work list|show`: current file awareness.
- `query workboard`: derived action queue.
- `maintenance digest --dry-run`: report cleanup.
- `repo inject`: regenerate projections while preserving plan folders.
