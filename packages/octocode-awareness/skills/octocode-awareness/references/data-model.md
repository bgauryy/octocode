# Awareness Data Model

Canonical DB: `~/.octocode/memory/awareness.sqlite3` (or
`$OCTOCODE_MEMORY_HOME/awareness.sqlite3`), canonical schema v1. `.octocode/` is projection
plus plan narrative, not operational state.

```text
plan -> tasks -> one claim/run -> run_files (advisory)
                              `-> locks (exclusive)
standalone work -> explicit WORK run -> same file/lock model
```

| Family | Tables |
|---|---|
| Plans | `plans`, `plan_members`, `plan_docs` |
| Tasks | `tasks`, `task_paths`, `task_dependencies`, `task_claims`, `task_events` |
| Execution | `task_runs`, `run_files`, `locks`, `run_log`, `edit_log` |
| Delivery | `delivery_state`, `signals`, `signal_reads` |
| Knowledge | `memories`, `memory_refs`, FTS, `refinements`, `harness_log` |
| Presence | `agents`, `sessions` |

Run origin is `TASK|WORK|HOOK`. Run files normalize active/historical path relation;
locks contain only exclusive protection. Derive agent/task/plan/reason through joins;
do not duplicate them per path/lock.

Readiness is derived from OPEN + no claim + completed dependencies. Tasks are the
only queue; refinements are owned follow-up. Details: `data-model-entities.md`;
joins: `data-model-relationships.md`. Inspect public operation contracts with
`schema commands --compact` and targeted `schema json-schema <name>`.
