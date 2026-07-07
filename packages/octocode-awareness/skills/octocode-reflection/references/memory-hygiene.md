# Memory Hygiene

Use this after work, before pruning, superseding, digesting, forgetting, or abandoning stale awareness state.

## Rules

- Preview before mutation: use dry-run output for broad cleanup.
- Prefer superseding a stale fact with a better current fact over hard deletion.
- Delete only wrong, duplicate, obsolete, stale, expired, or misleading memories.
- Never delete secrets by quoting them back; identify rows by id or safe metadata.
- Broad cleanup must respect importance. High-importance memories need explicit ids or a clear `max_importance` ceiling.
- During live work, keep `octocode-awareness` scoped to the current agent/task. Broad historical cleanup belongs here after the outcome is known.

## Commands

Recall before cleanup:

```bash
node <skill_root>/scripts/awareness.mjs get-memory --query "topic" --smart --limit 8
```

Supersede stale knowledge:

```typescript
memory_record({
  task_context: "replace stale repo lesson",
  observation: "current verified behavior",
  supersedes: ["mem_old"],
  references: ["file:/abs/current.ts:42"],
})
```

Preview deletion:

```bash
node <skill_root>/scripts/awareness.mjs forget --tag obsolete --dry-run
node <skill_root>/scripts/awareness.mjs forget --before 2026-01-01T00:00:00Z --max-importance 4 --dry-run
```

Apply only after the preview matches the intent:

```bash
node <skill_root>/scripts/awareness.mjs forget --memory-id mem_abc123
```

Use digest for retention cleanup:

```bash
node <skill_root>/scripts/awareness.mjs digest --dry-run
```

## Pending Task Cleanup

Use this when `workspace-status` or a global `audit-unverified --workspace <repo>` reports historical pending tasks from old agents.

During work, avoid global cleanup noise:

```bash
node <skill_root>/scripts/awareness.mjs audit-unverified --agent-id <current-agent> --workspace <repo>
```

After work, summarize the stale scope before mutating:

```bash
node <skill_root>/scripts/awareness.mjs workspace-status --workspace <repo>
node <skill_root>/scripts/awareness.mjs audit-unverified --agent-id <stale-agent> --workspace <repo>
```

Abandon only after the user approves the stale-agent scope:

```bash
node <skill_root>/scripts/awareness.mjs audit-unverified --agent-id <stale-agent> --workspace <repo> --abandon
```

Do not use unscoped `--abandon` unless the user explicitly approves clearing every pending task in that workspace.

## Stale/Expired Memory Checklist

1. Verify the memory is stale against current files, tests, docs, or user correction.
2. Choose supersede, forget-by-id, or digest dry-run.
3. Preview broad cleanup before applying.
4. Re-run targeted recall or status to confirm the misleading row is gone or superseded.
5. Record a concise lesson only if the cleanup rule itself is reusable.
