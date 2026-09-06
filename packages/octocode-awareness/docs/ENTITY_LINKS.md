# Entity identities and connections

Use this reference when tracing a record by ID. Discover the live inventory with
`schema entities --all --compact`. Entity names and constraints come from
[canonical DDL](../src/db-schema.ts), [continuity DDL](../src/db-continuity-schema.ts),
and the [worker projection](../src/worker-lifecycle-ledger.ts).

IDs are opaque handles. Canonical writers generate prefixed UUIDs; continuity and host IDs follow their owning contracts. Do not infer ownership from a prefix. Root commands and host APIs resolve the same records and relationships.

## Canonical ledger

Arrows in the foreign-key column are SQLite constraints. Composite keys are
shown in parentheses. A dash means no declared foreign key.

| Relation | Primary key | Foreign-key targets | Other connections |
|---|---|---|---|
| `sessions` | `session_id` | — | Opaque `agent_id`; workspace and artifact scope. |
| `awareness_agents` | (`workspace_path`, `agent_id`) | — | Workspace-scoped identity registry; actors need not have a registry row. |
| `awareness_plans` | `plan_id` | — | `lead_agent_id` is an actor handle; workspace/source keys fence idempotent graph imports. |
| `plan_members` | (`plan_id`, `agent_id`) | `plan_id` → `awareness_plans` | Membership joins actor identity to a plan. |
| `plan_docs` | (`plan_id`, `relative_path`) | `plan_id` → `awareness_plans` | Workspace-relative document path. |
| `awareness_tasks` | `task_id` | `plan_id` → `awareness_plans` | Workspace comes from the plan. |
| `task_paths` | (`task_id`, `path`) | `task_id` → `awareness_tasks` | Planned file scope. |
| `task_dependencies` | (`task_id`, `depends_on_task_id`) | Both IDs → `awareness_tasks` | Same-plan and acyclic graph checks belong to the task API. |
| `task_runs` | `run_id` | `task_id` → `awareness_tasks`; `session_id` → `sessions` | WORK and HOOK runs can have no task; session can be absent. |
| `run_files` | (`run_id`, `file_path`) | `run_id` → `task_runs` | Advisory work presence. |
| `task_claims` | `task_id` | `task_id` → `awareness_tasks`; unique `run_id` → `task_runs` | The claim's task and actor must agree with its run. |
| `task_events` | `event_id` | `task_id` → `awareness_tasks`; `run_id` → `task_runs` | A linked run must concern the event's task. |
| `awareness_locks` | `lock_id` | `run_id` → `task_runs` | File/run pair is unique; exclusive work protection. |
| `run_log` | `event_id` | `run_id` → `task_runs` | Actor and event metadata. |
| `awareness_memories` | `memory_id` | — | `superseded_by` is a historical replacement ID. |
| `memory_refs` | (`memory_id`, `reference`) | `memory_id` → `awareness_memories` | Reference text can identify files, commits, URLs, or external evidence. |
| `memories_fts` | No primary key | — | Search projection keyed logically by `awareness_memories.memory_id`; optional FTS shadow tables are internal. |
| `refinements` | `refinement_id` | — | Actor, workspace, artifact, and file references. |
| `signals` | `signal_id` | — | `reply_to` points to a parent; `thread_id` identifies its root. The signal API owns these links. |
| `signal_reads` | (`signal_id`, `agent_id`) | `signal_id` → `signals` | Actor-specific acknowledgement. |
| `delivery_state` | (`consumer_id`, `channel`, `scope_key`) | — | Consumer delivery fingerprint, not an event foreign key. |
| `hook_receipts` | (`workspace_path`, `host`, `event`) | — | Latest hook execution observation. |
| `edit_log` | `edit_id` | `session_id` → `sessions`; `run_id` → `task_runs` | Audit context includes actor, workspace, and file. |
| `harness_log` | `harness_id` | `session_id` → `sessions`; `memory_id` → `awareness_memories`; `run_id` → `task_runs` | Audit context and event-specific payload. |

## Continuity and host audit

These are delivery, interaction, and audit records, not additional work lifecycles.

| Relation | Primary key | Foreign-key targets | Other connections |
|---|---|---|---|
| `handoffs` | `handoff_id` | — | Actor, workspace, and file references. |
| `event_outbox` | `sequence` | — | Unique `event_id`; aggregate kind/ID, session, and correlation identify external or local subjects. |
| `event_consumers` | (`workspace_path`, `consumer_id`) | — | Sequence is a cursor, not a foreign key; pruning can remove earlier events. |
| `event_acknowledgements` | (`event_id`, `consumer_id`) | `event_id` → `event_outbox` | Consumer handles are workspace-scoped by the API. |
| `pending_interactions` | `interaction_id` | — | Unique `correlation_id`; host session and structured request/answer. |
| `authorization_receipts` | `receipt_id` | — | API checks the answered interaction and session; plan/revision/scope bind authorization rather than implying a local plan FK. |
| `capability_receipts` | `receipt_id` | — | Structured capability decision with host provenance. |
| `worker_lifecycle_events` | `sequence` | — | Unique `packet_id`; host-owned worker/session/correlation handles, indexed together with workspace. |

## Lease and completion fencing

Claims, run files, and exclusive locks carry expiry. Reads project expired state without deleting it. Reclaiming work creates a new run; heartbeat, release, submit, and verification must target the owned run. Host check receipts also bind the pending run's update timestamp. Expiry never proves success. A retry cannot reuse an old completion receipt.

Signals own peer threads and read receipts. Their outbox event is inserted atomically with the signal. Consumer cursors describe transport progress; they do not replace participant read acknowledgements.

## What an integrity check proves

`PRAGMA foreign_key_check` detects missing targets for declared constraints. It
does not validate workspace equality, actor ownership, dependency graphs,
historical replacement IDs, JSON references, or host-owned IDs. Test those
through the owning APIs, including mismatched IDs from another workspace.

Deleting a memory can leave its ID in older `superseded_by` history. Treat an
unresolved replacement as a retained historical reference. Clearing it blindly
makes replaced memories look like archived memories eligible for restore.
Audit writers likewise retain caller-supplied context; foreign keys establish
target existence, not agreement between every actor, session, run, and workspace.

See [database identity and storage checks](DB.md) before opening an existing
store. Never repair an ID by guessing from a prefix or rewriting live rows.
