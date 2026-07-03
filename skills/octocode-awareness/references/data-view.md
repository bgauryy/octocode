# Awareness Data Viewer

**Invoke when the user asks to see, browse, view, or audit awareness data** — memories, locks, intents, refinements. Never dump raw rows into chat; run the commands below and summarise.

## Status snapshot

```bash
# Full JSON: memory counts, labels, active locks, open refinements
node <skill_root>/scripts/awareness.mjs status

# Compact (single line — pipe-friendly)
node <skill_root>/scripts/awareness.mjs status --compact

# Specific store
node <skill_root>/scripts/awareness.mjs --db ~/.octocode/memory/awareness.sqlite3 status
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

# Include done
node <skill_root>/scripts/awareness.mjs refine-get --state open --state done
```

## Filtering with jq

```bash
node <skill_root>/scripts/awareness.mjs status | jq '.memory_labels'
node <skill_root>/scripts/awareness.mjs status | jq '.locks[] | select(.agent_id == "my-agent")'
node <skill_root>/scripts/awareness.mjs status | jq '.open_refinements'
```

## One shared store

Everything lives in `~/.octocode/memory/awareness.sqlite3` (relocate with `OCTOCODE_MEMORY_HOME`). The `status` command reads all live tables and auto-prunes expired file locks on each call.
