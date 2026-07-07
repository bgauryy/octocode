# Harness Lifecycle

Use this when evaluating harness gates, reflection loops, session capture, doc staleness, or self-improvement storage.
The harness records evidence; humans approve prompt, skill, or repo changes.

## Loop

```text
mine -> propose -> validate -> apply
        ^             ^
        |             |
      capture       reflect
```

| Event | Purpose | Writer today |
|---|---|---|
| `mine` | Find recurring failure signatures | `mine-weakness` view |
| `propose` | Draft a bounded improvement | `doc-staleness --propose`, module API |
| `validate` | Record why a proposal works or fails | module API |
| `apply` | Record an approved handoff/refinement | module API |
| `capture` | Preserve session-end unresolved work | `session-capture` |
| `reflect` | Store post-task outcome | `reflect` |

There is no generic `harness-run` CLI. Use the CLI commands above or module APIs `insertHarnessLog()` / `queryHarnessLog()`.

## Session Lifecycle

`sessions` is available through module APIs: `insertSession()`, `getOrCreateSession()`, `endSession()`.
CLI session support is module-first: most CLI commands omit a session flag.
`awareness session-capture` is different: it snapshots unresolved `ACTIVE`/`PENDING` tasks plus dirty worktree context into a handoff refinement.

```sh
awareness session-capture --agent-id <id> --workspace <path> --reason "context switch"
```

Use `sessions` when embedding the library in a host. Use `session-capture` when a shell hook or agent run needs a durable handoff.

## Edit Audit

`edit_log` exists for audit and doc-staleness, but the guaranteed public path today is the module API: `insertEditLog()` and `queryEditLog()`.
Hook-managed edits release locks as `PENDING`; do not assume every host has complete diff stats in `edit_log`.

Important `edit_log` fields: `edit_id`, `session_id`, `task_id`, `agent_id`, `file_path`, `operation`, `old_file_path`, `lines_added`, `lines_removed`, `content_hash`, `workspace_path`, `artifact`, `created_at`.

## Pre-Flight Cycle

```sh
TASK=$(awareness pre-flight-intent \
  --agent-id <id> \
  --target-file src/foo.ts \
  --rationale "change reason" \
  --test-plan "test command" | jq -r '.task.task_id')

# edit files
awareness release-file-lock --agent-id <id> --task-id "$TASK" --status PENDING
# run test command
awareness verify --agent-id <id> --task-id "$TASK" --status SUCCESS --message "test output"
```

If verification fails, use `--status FAILED`.
If a hook releases locks, run `verify --workspace <root> --all-pending` after the declared checks.
The stop hook runs `audit-unverified` and blocks shell-hook hosts once while verification debt remains.

## Harness Queries

```sql
-- Recurring failure signatures
SELECT failure_signature, COUNT(*) AS freq, AVG(importance) AS avg_imp
FROM memories
WHERE failure_signature IS NOT NULL AND state = 'ACTIVE'
GROUP BY failure_signature
HAVING freq >= 2;

-- Open harness proposals
SELECT event_type, payload_json, created_at
FROM harness_log
WHERE event_type IN ('propose', 'validate', 'apply')
ORDER BY created_at DESC;

-- Unverified tasks older than 1 hour
SELECT task_id, agent_id, rationale, updated_at
FROM tasks
WHERE status = 'PENDING'
  AND updated_at < datetime('now', '-1 hour');
```

## Doc Staleness

`doc-staleness` compares source edit activity against the doc file's last `edit_log` timestamp.
Doc staleness is opt-in per `{ docFile, sourceDirs }` mapping.

```sh
awareness doc-staleness \
  --targets-json '[{"docFile":"packages/foo/ARCHITECTURE.md","sourceDirs":["packages/foo/src"]}]' \
  --workspace <path> --artifact <name> --min-edits 5 --min-lines 50
```

Add `--propose --agent-id <id>` to record a `harness_log` `propose` event for stale docs.
Defaults treat small churn as normal: `--min-edits 5`, `--min-lines 50`.

## OVERRIDE Memories

Use label `OVERRIDE` only for critical invariants that contradict default model assumptions.
They surface in briefing regardless of importance floor, decay, or normal salience.

```sh
awareness tell-memory --agent-id <id> --label OVERRIDE --importance 10 \
  --task-context "repo invariant" \
  --observation "Never use Date.now() directly for JWT exp; convert ms to seconds."
```

## Gate

Before changing `AGENTS.md`, `CLAUDE.md`, this skill, or standing memory guidance:
- Name the failure or opportunity.
- Name the exact target file.
- Provide evidence from memory, test, eval, user correction, or source.
- Explain verification and rollback.
- Wait for human approval for that scoped change.

After the change, run skill lint, package tests, and the relevant smoke command.

For awareness-store maintenance, preview before mutation and record the observed result.
Use `legacy-migrate.mjs` summaries for legacy tables, `forget --dry-run` for stale current memories, and `digest --dry-run` for expired/superseded cleanup.
Verify with `status`, targeted `get-memory`, or the relevant package test before claiming success.
