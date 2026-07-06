---
name: octocode-awareness
description: "Use when coding needs shared memory, file locks, agent handoffs, notifications, or verify-before-conclude for concurrent edits."
hooks:
  PreToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit", hooks: [{ type: command, command: "$SKILL_DIR/scripts/hooks/pre-edit.sh", timeout: 20 }, { type: command, command: "$SKILL_DIR/scripts/hooks/harness-guard.sh", timeout: 20 }] }]
  PostToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit", hooks: [{ type: command, command: "$SKILL_DIR/scripts/hooks/post-edit.sh", timeout: 20 }] }]
  Stop: [{ hooks: [{ type: command, command: "$SKILL_DIR/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SubagentStop: [{ hooks: [{ type: command, command: "$SKILL_DIR/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SessionEnd: [{ hooks: [{ type: command, command: "$SKILL_DIR/scripts/hooks/session-end.sh", timeout: 20 }] }]
  UserPromptSubmit: [{ hooks: [{ type: command, command: "$SKILL_DIR/scripts/hooks/notify-deliver.sh", timeout: 20 }] }]
---
# Octocode Awareness
Local SQLite memory, locks, notifications, session handoffs, and verify-before-conclude. Store: `~/.octocode/memory/awareness.sqlite3`, scoped by workspace/repo/ref.

> **Pi tools:** `workspace_status` · `memory_recall` · `memory_refine_get` · `agent_signal` · `file_lock` · `memory_record` · `memory_reflect` · `memory_verify` · `memory_digest` · `memory_forget` · `memory_audit_unverified`. Hook scripts use `scripts/awareness.mjs <cmd>`.

## Agent loop
1. **Attend** — `workspace_status`, `memory_recall`, `memory_refine_get`, `agent_signal action:list`; validate recalled facts against current files.
2. **Claim** — `file_lock type:lock target_files:[<abs-path>]`; on conflict stop or `wait_for_lock` via CLI.
3. **Work** — edit under the lock; record durable findings with `memory_record`.
4. **Verify** — run the declared test plan; clear with `memory_verify allPending:true` or `file_lock type:release verified:true`.
5. **Encode** — `memory_record`/`memory_reflect` for reusable lessons; `agent_signal` for live coordination.
6. **Sleep** — `memory_reflect`, then `memory_digest dry_run:true`; run `memory_digest` when pruning is safe.

## References
- `references/memory-recall.md` — when recording, recalling, labeling, superseding, or capturing research conclusions.
- `references/coordination-protocol.md` — when using locks, waits, releases, refinements, and notifications.
- `references/files-awareness.md` — when dirty state or concurrent edits may collide.
- `references/hooks.md` — before installing, auditing, tuning, or removing hooks.
- `references/self-harness.md` — before proposing or applying approved harness/skill changes.
- `references/brain-model.md` — when tuning recall, salience, cleanup, or sleep behavior.
- `references/agentic-flows.md` — when composing hooks, handoffs, subagents, and cleanup.
- `references/corpus.md` — when maintaining `~/.octocode/awareness/corpus/` notes.
- `references/data-view.md` — when showing, viewing, or pruning awareness data on request.
- `references/octocode.md` — when choosing Octocode MCP vs CLI for code research.

## Scripts
- `scripts/awareness.mjs` — memory, lock, verification, refinement, notification, digest, and session-capture commands.
- `scripts/install-hooks.mjs` — preview/install/remove file-lock hooks; get approval before writes.
- `scripts/hook-runner.mjs` — lifecycle dispatcher used by shell hooks; inspect when debugging hook behavior.
- `scripts/extract-hook-files.mjs` — inspect write-path extraction when adding new host tool support.
- `scripts/prune-stale-locks.sh` — lock cleanup for cron or shell automation outside hook scope.
- `scripts/install.mjs` — check local dependencies before relying on them; run with --check-only.
- `scripts/schema.mjs` — inspect JSON payload contracts before building wrappers or MCP adapters.
- `scripts/smoke-multi-agent.mjs` — verify locks, notifications, verification, release, and stale-prune behavior.

## Installation
Preview: `node <skill_root>/scripts/install-hooks.mjs --dry-run --project-dir <repo>`.
