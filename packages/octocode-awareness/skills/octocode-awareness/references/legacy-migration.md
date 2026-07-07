# Legacy Migration

Use this when `status` shows zero current memories but the SQLite store still has old tables such as `agent_memories`, `memory_fts`, `agent_intents`, `file_locks`, or `notifications`.

Current runtime reads current tables only. Migrate legacy rows with the bundled skill script; keep this flow out of `awareness.mjs` and the package CLI.

The script copies only old memory rows into the current `memories`/`memory_refs`/`memories_fts` tables. Old task, lock, signal, notification, identity, and meta tables are either left alone or dropped with `--write --drop-legacy` after verification.

## Flow

1. Preview:

```bash
node <skill_root>/scripts/legacy-migrate.mjs
```

2. Copy old `agent_memories` into current `memories`, `memory_refs`, and `memories_fts`:

```bash
node <skill_root>/scripts/legacy-migrate.mjs --write
```

3. Verify current recall:

```bash
node <skill_root>/scripts/awareness.mjs status
node <skill_root>/scripts/awareness.mjs get-memory --query "<known old topic>" --smart --limit 5
```

4. Remove old tables only after the copy and recall check look right:

```bash
node <skill_root>/scripts/legacy-migrate.mjs --write --drop-legacy
```

## Rules

- Run preview first; inspect-only mode must not mutate the DB.
- `--write` skips memory ids already present in `memories`, so it is safe to retry.
- `--drop-legacy` is refused unless `--write` is also present; use it after verification, not as the first step.
- If the DB path is not the default, pass `--db /abs/path/to/awareness.sqlite3`.
