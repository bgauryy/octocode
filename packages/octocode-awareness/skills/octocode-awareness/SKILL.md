---
name: octocode-awareness
description: "Use when starting, planning, editing, messaging, reflecting, finishing, or managing repo awareness in a shared workspace. Run the compact Awareness CLI for memory recall, file locks, signals, verification, hooks, repo context, and durable lessons."
hooks:
  PreToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/pre-edit.sh", timeout: 20 }, { type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/harness-guard.sh", timeout: 20 }] }]
  PostToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/post-edit.sh", timeout: 20 }] }]
  Stop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SubagentStop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SessionEnd: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/session-end.sh", timeout: 20 }] }]
  UserPromptSubmit: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/notify-deliver.sh", timeout: 20 }] }]
---
# Octocode Awareness
Use this as the single operational awareness skill. It owns memory, file locks, verification, signals, reflection, hooks, and `.octocode/` repo context projections. Store: `~/.octocode/memory/awareness.sqlite3`, scoped by workspace, optional artifact/package/service, repo, and ref.

First command: `npx @octocodeai/octocode-awareness schema commands --compact` (or `scripts/awareness.mjs ...` inside a standalone skill folder). Then use `<command> --help --compact` or `schema json-schema <name> --compact` for exact flags/contracts. Use only canonical noun/verb commands. Pi exposes equivalent methods.

## CLI Workflow
- Start: `workspace status`, `memory recall`, `refinement get`, `signal list`.
- Edit/verify: `lock acquire|wait|release`, `verify audit|mark`.
- Message/learn: `signal publish|list|reply|ack|resolve`, `agent register|list`, `memory record|forget`, `reflect record|mine-weakness`, `maintenance digest`.
- Repo/hooks/inspect: `query <view>`, `repo inject`, `docs staleness`, `session capture`, `hooks install|check|remove`, `hook run`, `schema commands|list|json-schema|example|validate`.

## Operating Loop
1. **Think / Plan** — run status, recall memories, read refinements, list signals when no hook briefing was delivered, and verify remembered facts against current files.
2. **Before Edits** — claim likely target files; on conflict wait, coordinate, switch to non-overlapping work, or stop.
3. **After Edits** — run the declared verification, mark verification, and release the claim.
4. **Messages** — handle relevant signals with publish/reply/ack/resolve; broadcast only when the recipient is unknown.
5. **Finish / Learn** — keep pending work visible, send handoffs, record durable lessons, and use `octocode-skills` for repeated repo-skill improvements.

If older prompts name `octocode-agent-communication` or `octocode-reflection`, load this skill for the actual workflow. Read `references/hooks.md` before installing hooks because Codex and Cursor require host config or plugin hooks, not `SKILL.md` frontmatter alone.

## References
- `references/memory-recall.md` — before planning, recall durable lessons and validate them against current files.
- `references/coordination-protocol.md` — before locking, waiting, releasing, signaling, or managing refinements.
- `references/files-awareness.md` — before planning/editing in dirty or concurrently edited workspaces.
- `references/hooks.md` — before installing, auditing, tuning, or removing hooks.
- `references/repo-context-management.md` — before generating, refreshing, sharing, or relying on `.octocode/` repo context projections.
- `references/data-model.md` — when checking SQLite schema, memory rows, tasks, locks, or signals.
- `references/octocode.md` — when code research is needed; delegates Octocode research rules to `octocode-research`.

## Installation
- `npx @octocodeai/octocode-awareness` — public package CLI; run `schema commands --compact` for the agent map, `<command> --help --compact` for flags, `schema` for contracts, `hook run` for hook events, and `hooks install|check|remove` for Claude/Codex/Cursor hook config.
- `scripts/awareness.mjs` — bundled standalone fallback with the same command surface.
- `scripts/schema.mjs`, `scripts/install.mjs`, `scripts/smoke-multi-agent.mjs` — inspect payload contracts, check dependencies, and smoke-test locks/signals/verification.
- `scripts/install-hooks.mjs`, `scripts/hook-runner.mjs`, `scripts/extract-hook-files.mjs` — hook install wrapper, lifecycle dispatcher, and write-path extractor.
Preview hook writes with `npx @octocodeai/octocode-awareness hooks install --host cursor --dry-run --project-dir <repo> --compact`, `--host codex`, or `--host claude`.
