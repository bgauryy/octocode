# Awareness Data Model

Load when changing schemas or interpreting stored entities.

SQLite is canonical. Awareness uses `$OCTOCODE_HOME/awareness/awareness.sqlite3` by default; an explicit policy or `--db-scope repo` selects `<workspace>/.octocode/awareness.sqlite3`, and `--db <path>` wins. Agent control and runtime databases are separate owners, and other `.octocode/` databases or generated files are not Awareness state.

```text
plan -> task -> claim/run -> run_files (advisory)
                           `-> awareness_locks (exclusive)
standalone work ---------> run -> same file/lock model
```

| Entity family | Live routes |
|---|---|
| Planning | `plan`, `task` |
| Execution and safety | `work`, `lock`, `verify` |
| Coordination | `agent`, `signal`, `session capture` |
| Learning | `memory`, `refinement`, `reflect` |
| Read and maintenance | `attend`, `status`, `query`, `docs`, `schema`, `maintenance`, `config`, `hooks` |

Run origin is `TASK|WORK|HOOK`. Task readiness is derived from `OPEN`, no live claim, and completed dependencies. Tasks are the queue; refinements are owned follow-up.

```text
task: OPEN -> IN_PROGRESS -> VERIFY -> DONE|FAILED
                    \-> OPEN|BLOCKED
run:  ACTIVE -> PENDING -> SUCCESS|FAILED
```

`task_runs` owns agent/session/task identity and rationale. `run_files` records advisory paths; `awareness_locks` records only exclusive protection; `edit_log` is completed history. Never conflate these layers.

Verification debt is a `PENDING` run. TTL, cleanup, and delivery fingerprints never establish success. Signal reads live in `signal_reads`; memory references preserve provenance.

Physical relation names and migrations belong to the executable DDL and introspection modules; do not infer them from this compact map. Inspect public contracts with `schema commands --compact`, physical ownership with `schema entities --compact`, and one command shape with `schema json-schema <name>`. The entity catalog is read-only and contains no stored rows.

IDs are opaque handles, not proof of ownership. Check the target entity, workspace, and relationship through its owning API; similar prefixes never establish relationships. Host IDs and historical references need not resolve to local rows. Foreign-key checks alone do not validate these links.

Next: return to `SKILL.md`; use `references/files-awareness.md` for overlap semantics.
