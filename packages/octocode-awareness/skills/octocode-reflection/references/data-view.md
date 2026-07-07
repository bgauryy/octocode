# Awareness Data Viewer

**Invoke when the user asks to see, browse, view, or audit awareness data** — memories, locks, tasks, refinements, and signals. Never dump raw rows into chat; run the commands below and summarise.

## Status snapshot

```bash
# Full JSON: memory counts, labels, active locks, open refinements
node <skill_root>/scripts/awareness.mjs status

# Lock/agent focused view (who is editing what)
node <skill_root>/scripts/awareness.mjs workspace-status --workspace /repo

# Compact (single line — pipe-friendly)
node <skill_root>/scripts/awareness.mjs status --compact
```

## Querying memories

```bash
# Recall by topic
node <skill_root>/scripts/awareness.mjs get-memory --query "auth router" --limit 5

# High-importance only
node <skill_root>/scripts/awareness.mjs get-memory --query "sqlite" --min-importance 7

# Filter by label
node <skill_root>/scripts/awareness.mjs get-memory --label BUG --min-importance 1
```

## Refinements (repo-fix queue)

```bash
# List open / ongoing
node <skill_root>/scripts/awareness.mjs refine-get

# Delete a stale refinement
node <skill_root>/scripts/awareness.mjs refine-delete --refinement-id ref_abc123 --dry-run
node <skill_root>/scripts/awareness.mjs refine-delete --refinement-id ref_abc123
```

## Forgetting memories

```bash
# Preview what would be deleted (dry-run always first)
node <skill_root>/scripts/awareness.mjs forget --max-importance 3 --dry-run
node <skill_root>/scripts/awareness.mjs forget --tag obsolete --dry-run

# Actually delete (add --before <ISO> for age filter)
node <skill_root>/scripts/awareness.mjs forget --memory-id mem_abc123
```

Use `forget` for wrong, stale, duplicate, or obsolete memories. Use `digest --dry-run` for expired/superseded retention cleanup.

## Signals (agent-to-agent messages)

```bash
# Send a message
node <skill_root>/scripts/awareness.mjs notify --agent-id agent-a --kind blocker \
  --subject "Hold off editing src/auth.ts" --workspace /repo

# Read inbox (unread only by default)
node <skill_root>/scripts/awareness.mjs notify-get --agent-id agent-b --workspace /repo

# Resolve and prune
node <skill_root>/scripts/awareness.mjs notify-resolve --thread-id ntf_abc123
node <skill_root>/scripts/awareness.mjs notify-prune --resolved --dry-run
```

## Harness lessons preview

```bash
# Preview top lessons as an AGENTS.md block (never writes files)
node <skill_root>/scripts/awareness.mjs export-harness --limit 10 --min-importance 7
```

## Filtering with jq

```bash
node <skill_root>/scripts/awareness.mjs status | jq '.memory_labels'
node <skill_root>/scripts/awareness.mjs workspace-status | jq '.locks'
node <skill_root>/scripts/awareness.mjs status | jq '.open_refinements'
```

## Digest retention defaults

`memory_digest` (CLI: `awareness.mjs digest`) prunes data older than these defaults; override with explicit flags:

| What | Default | Flag |
|------|---------|------|
| SUPERSEDED memories | 90 days | `retention_days` |
| Session handoff refinements | 7 days | `refinement_handoff_retention_days` |
| Completed (`done`) repo-fix refinements | 30 days | `refinement_done_retention_days` |

All use `updated_at` for the age calculation. Run with `--dry-run` first to preview what would be pruned.

## One shared store

Everything lives in `~/.octocode/memory/awareness.sqlite3` (relocate with `OCTOCODE_MEMORY_HOME`). The `status` and `workspace-status` commands auto-prune expired file locks on each call.
