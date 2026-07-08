---
name: octocode-awareness
description: "Use when starting, planning, editing, messaging, reflecting, or finishing in a shared workspace. Recall memory, inspect status, claim locks, handle signals, track verification, and record durable lessons."
hooks:
  PreToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/pre-edit.sh", timeout: 20 }, { type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/harness-guard.sh", timeout: 20 }] }]
  PostToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/post-edit.sh", timeout: 20 }] }]
  Stop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SubagentStop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SessionEnd: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/session-end.sh", timeout: 20 }] }]
  UserPromptSubmit: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/notify-deliver.sh", timeout: 20 }] }]
---
# Octocode Awareness
Use this as the live workspace awareness loop whenever local files, plans, or conclusions may affect another agent or a later run.
Store: `~/.octocode/memory/awareness.sqlite3`, scoped by workspace, optional artifact/package/service, repo, and ref.
Octocode Awareness is the primary skill for awareness, communication, reflection, learning, and hook guidance. `octocode-agent-communication` and `octocode-reflection` are compatibility stubs that route back here.

Use `scripts/awareness.mjs <command> --help` and `scripts/awareness.mjs schema list|json-schema|example` for contracts. Prefer the noun/verb CLI (`memory recall`, `lock acquire`, `signal list`, `reflect record`, `hooks install`); legacy flat commands remain aliases for the transition release. Pi exposes equivalent awareness methods. Hook scripts call `scripts/awareness.mjs hook run <event>`.

## Default Loop
1. **Think / Plan** — check status, recall memories, read refinements, run `signal list`/`notify-get --agent-id <id> --workspace <repo> --compact` when no hook briefing was delivered, and validate facts against current files before deciding.
2. **Before Edits** — claim every likely target file; on conflict wait, coordinate, switch to non-overlapping work, or stop.
3. **After Edits** — run the declared verification, clear your scoped pending tasks, and release/verify the claim.
4. **Messages** — publish, list, reply, ack, and resolve signals with `signal publish|list|reply|ack|resolve`; use broadcast when the recipient is unknown.
5. **Finish / Learn** — resolve or send handoffs/signals, keep pending work visible, record lessons with `memory record`/`reflect record`, and use `octocode-skills` to turn repeated work into linted repo skills under `skills/`.

Hooks can enforce the loop around write tools when the host is wired for them.
Read `references/hooks.md` before changing or installing hooks; Codex and Cursor require host config or plugin hooks, not `SKILL.md` frontmatter alone.
Message routing: if `notify-get` or hook-injected briefing shows unread or relevant agent messages, handle them here with the signal commands and `references/coordination-protocol.md`.

## References
- `references/memory-recall.md` — before planning, recall durable lessons and validate them against current files.
- `references/coordination-protocol.md` — before locking, waiting, releasing, signaling, or managing refinements.
- `references/files-awareness.md` — before planning/editing in dirty or concurrently edited workspaces.
- `references/hooks.md` — before installing, auditing, tuning, or removing hooks.
- `references/repo-context-management.md` — before generating, refreshing, sharing, or relying on `.octocode/` repo context projections.
- `references/data-model.md` — when checking SQLite schema, memory rows, tasks, locks, or signals.
- `references/octocode.md` — when code research is needed; delegates Octocode research rules to `octocode-research`.
Compatibility skill names (`octocode-agent-communication`, `octocode-reflection`) route back here and do not own separate operational references.

## Scripts
- `scripts/awareness.mjs` — shared generated CLI; run it with `--help` for commands, `schema` for contracts, `hook run` for hook events, and `hooks install|check|remove` for Claude/Codex/Cursor hook config.
- `scripts/install-hooks.mjs` — compatibility wrapper for `scripts/awareness.mjs hooks install`; get approval before writes.
- `scripts/hook-runner.mjs` — compatibility lifecycle dispatcher used by shell hooks; inspect when debugging hook behavior.
- `scripts/extract-hook-files.mjs` — inspect write-path extraction when adding new host tool support.
- `scripts/prune-stale-locks.sh` — lock cleanup for cron or shell automation outside hook scope.
- `scripts/install.mjs` — check local dependencies before relying on them; run with --check-only.
- `scripts/schema.mjs` — inspect JSON payload contracts before building wrappers or MCP adapters.
- `scripts/smoke-multi-agent.mjs` — verify locks, signals, verification, release, and stale-prune behavior.
## Installation — preview hook writes with `node <skill_root>/scripts/awareness.mjs hooks install --host cursor --dry-run --project-dir <repo>`, `--host codex`, or `--host claude`.
