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

## Agent loop
1. **Attend** — run `workspace-status`, `get-memory`, `refine-get`, and `notify-get`; validate recalled facts against current files.
2. **Claim** — before writes, call `pre-flight-intent --target-file <abs-path>`; exit `2` means stop or wait with `wait-for-lock`.
3. **Work** — edit under the lock; record durable findings immediately with `tell-memory`.
4. **Verify** — run the declared test plan; clear pending work with `verify --all-pending` or `release-file-lock --verified`.
5. **Encode** — use memories for reusable lessons, refinements for repo-fix queues, notifications for live coordination.
6. **Sleep** — run `reflect`, `digest --dry-run`, then `digest` when pruning is safe; `session-capture` writes unresolved session handoffs.

## References
- `references/memory-recall.md` — when recording, recalling, labeling, superseding, or indexing memories.
- `references/learning-capture.md` — when storing research conclusions with source references.
- `references/coordination-protocol.md` — when using locks, waits, releases, refinements, and notifications.
- `references/files-awareness.md` — when dirty state or concurrent edits may collide.
- `references/hooks.md` — before installing, auditing, tuning, or removing hooks.
- `references/self-harness.md` — before proposing or applying approved harness/skill changes.
- `references/brain-model.md` — when tuning recall, salience, cleanup, or sleep behavior.
- `references/agentic-flows.md` — when composing hooks, handoffs, subagents, and cleanup.
- `references/corpus.md` — when maintaining curated `~/.octocode/awareness/corpus/` notes.
- `references/data-view.md` — when showing, viewing, or pruning awareness data on request.
- `references/octocode.md` — when choosing Octocode MCP vs CLI for code research.
- `references/similar-systems.md` — when comparing or redesigning agent-memory systems.

## Scripts
- `scripts/awareness.mjs` — run memory, lock, verification, refinement, notification, digest, and session-capture commands.
- `scripts/schema.mjs` — inspect or validate JSON payload contracts before building wrappers.
- `scripts/install.mjs` — run `--check-only` before relying on local dependencies.
- `scripts/install-hooks.mjs` — preview/install/remove file-lock hooks; get approval before writes.
- `scripts/hook-runner.mjs` — lifecycle dispatcher used by shell hooks; inspect when debugging hook behavior.
- `scripts/extract-hook-files.mjs` — inspect hook payload path extraction when adding host support.
- `scripts/prune-stale-locks.sh` — run lock cleanup from cron or shell automation.
- `scripts/smoke-multi-agent.mjs` — verify locks, notifications, pending verification, release, and stale-prune behavior.

## Installation
Preview hook changes with `node <skill_root>/scripts/install-hooks.mjs --dry-run --project-dir <repo>`. Runtime check: `node <skill_root>/scripts/install.mjs --check-only`.
