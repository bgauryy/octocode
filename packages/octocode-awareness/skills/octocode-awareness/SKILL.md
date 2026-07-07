---
name: octocode-awareness
description: "Use when thinking, before planning, before/after editing, or finishing in a shared workspace. Coordinate recall, locks, signals, handoffs, and verification gates."
hooks:
  PreToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/pre-edit.sh", timeout: 20 }, { type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/harness-guard.sh", timeout: 20 }] }]
  PostToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/post-edit.sh", timeout: 20 }] }]
  Stop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SubagentStop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SessionEnd: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/session-end.sh", timeout: 20 }] }]
  UserPromptSubmit: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/notify-deliver.sh", timeout: 20 }] }]
---
# Octocode Awareness
Use this as the live workspace awareness loop whenever local files, plans, or conclusions may affect another agent or a later run. Store: `~/.octocode/memory/awareness.sqlite3`, scoped by workspace, optional artifact/package/service, repo, and ref.

Pi tools mirror the CLI: `workspace_status`, `memory_recall`, `memory_refine_get`, `agent_signal`, `file_lock`, `memory_verify`, `memory_audit_unverified`. Hook scripts call `scripts/awareness.mjs <cmd>`.

## Default Loop
1. **Think / Plan** — check status, recall memories, read refinements/signals, and validate facts against current files before deciding.
2. **Before Edits** — claim every likely target file; on conflict wait, coordinate, switch to non-overlapping work, or stop.
3. **After Edits** — run the declared verification, clear your scoped pending tasks, and release/verify the claim.
4. **Finish** — resolve or send handoffs/signals, leave pending work visible, then use `octocode-reflection` for lessons or stale global cleanup.

Hooks can enforce the loop around write tools when the host is wired for them.
Read `references/hooks.md` before changing or installing hooks; Codex requires `.codex` hook config or plugin hooks, not `SKILL.md` frontmatter alone.

## References
- `references/memory-recall.md` — before planning, recall durable lessons and validate them against current files.
- `references/coordination-protocol.md` — before locking, waiting, releasing, signaling, or managing refinements.
- `references/files-awareness.md` — before planning/editing in dirty or concurrently edited workspaces.
- `references/hooks.md` — before installing, auditing, tuning, or removing hooks.
- `references/data-model.md` — when checking SQLite schema, memory rows, tasks, locks, or signals.
- `references/octocode.md` — when choosing Octocode MCP vs CLI for code research.

## Scripts
- `scripts/awareness.mjs` — shared CLI; this skill uses recall, lock, verification, refinement, signal, and session-capture commands.
- `scripts/install-hooks.mjs` — preview/install/remove Claude or Codex lifecycle hooks; get approval before writes.
- `scripts/hook-runner.mjs` — lifecycle dispatcher used by shell hooks; inspect when debugging hook behavior.
- `scripts/extract-hook-files.mjs` — inspect write-path extraction when adding new host tool support.
- `scripts/prune-stale-locks.sh` — lock cleanup for cron or shell automation outside hook scope.
- `scripts/install.mjs` — check local dependencies before relying on them; run with --check-only.
- `scripts/schema.mjs` — inspect JSON payload contracts before building wrappers or MCP adapters.
- `scripts/smoke-multi-agent.mjs` — verify locks, signals, verification, release, and stale-prune behavior.
## Installation — preview hook writes with `node <skill_root>/scripts/install-hooks.mjs --host codex --dry-run --project-dir <repo>` or `--host claude` for Claude Code.
