# Awareness entity links

The public entity inventory groups canonical SQLite relations by owner. Run `npx @octocodeai/octocode-awareness schema entities --compact` for the machine-readable list.

| Family | Entities | Primary relationships |
|---|---|---|
| Storage | `awareness_meta` | Identifies the schema and stable store |
| Identity | `awareness_agents` | Names actors referenced by coordination records |
| Presence | `sessions` | Binds an actor and session to a physical workspace |
| Planning | `awareness_plans`, `plan_members`, `plan_docs` | Members and documents belong to one plan |
| Tasks | `awareness_tasks`, `task_paths`, `task_dependencies` | Tasks belong to plans; dependencies connect tasks |
| Execution | `task_runs`, `run_files`, `task_claims` | Attempts own file presence; a live claim selects one task attempt |
| Locks | `awareness_locks` | Exceptional path protection belongs to an actor and attempt |
| Messaging | `signals`, `signal_reads` | Replies retain a thread root; reads are actor-specific |
| Delivery | `delivery_state` | Stores channel and consumer fingerprints or cursors |
| Events | `event_outbox`, `event_consumers`, `event_acknowledgements` | Ordered events are acknowledged per consumer after delivery |
| Memory | `awareness_memories`, `memory_refs` | References ground one scoped memory |
| Search | `memories_fts` | Optional search projection over Memory |
| History | `local_history_operations`, `local_history_versions`, `local_history_restores`, `local_history_durability` | Operations own versions; restores and durability track recovery state |
| Hooks | `hook_receipts` | Records lifecycle outcome as success, degraded, or failure |
| Interactions | `pending_interactions` | Tracks host-mediated interaction state |
| Authorization | `authorization_receipts`, `capability_receipts` | Records trusted authorization and capability decisions |

## Invariants

- Deleting or expiring ownership cannot manufacture a successful verification receipt.
- A Message read, delivery acknowledgement, and thread resolution are separate facts.
- Memory references can point to source or LocalGit evidence, but the Memory remains a lead.
- LocalGit objects are external to SQLite; History relations store identities, digests, refs, and journals.
- Event sequence and acknowledgement state support restart-safe host consumption.

SQL relation names are an internal persistence contract. Routine callers use Context, Work, Message, Memory, and History operations instead of querying tables directly.
