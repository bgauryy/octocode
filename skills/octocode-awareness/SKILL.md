---
name: octocode-awareness
description: "Use when coding needs brain-like memory layers, locks, and verified handoff for concurrent agents: recall/record lessons, claim files before edits, consolidate work into refinements/docs, and verify-before-conclude. Trigger before dirty/concurrent edits, overlap risk, handoffs, cleanup, or post-work verification."
hooks:
  PreToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit", hooks: [{ type: command, command: "sh -c 'd=\"${CLAUDE_SKILL_DIR}\"; [ -n \"$d\" ] || d=\"${CLAUDE_PROJECT_DIR}/.claude/skills/octocode-awareness\"; s=\"$d/scripts/hooks/pre-edit.sh\"; [ -x \"$s\" ] && exec \"$s\" || exit 0'", timeout: 20 }, { type: command, command: "sh -c 'd=\"${CLAUDE_SKILL_DIR}\"; [ -n \"$d\" ] || d=\"${CLAUDE_PROJECT_DIR}/.claude/skills/octocode-awareness\"; s=\"$d/scripts/hooks/harness-guard.sh\"; [ -x \"$s\" ] && exec \"$s\" || exit 0'", timeout: 20 }] }]
  PostToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit", hooks: [{ type: command, command: "sh -c 'd=\"${CLAUDE_SKILL_DIR}\"; [ -n \"$d\" ] || d=\"${CLAUDE_PROJECT_DIR}/.claude/skills/octocode-awareness\"; s=\"$d/scripts/hooks/post-edit.sh\"; [ -x \"$s\" ] && exec \"$s\" || exit 0'", timeout: 20 }] }]
  Stop: [{ hooks: [{ type: command, command: "sh -c 'd=\"${CLAUDE_SKILL_DIR}\"; [ -n \"$d\" ] || d=\"${CLAUDE_PROJECT_DIR}/.claude/skills/octocode-awareness\"; s=\"$d/scripts/hooks/stop-verify.sh\"; [ -x \"$s\" ] && exec \"$s\" || exit 0'", timeout: 20 }] }]
  SubagentStop: [{ hooks: [{ type: command, command: "sh -c 'd=\"${CLAUDE_SKILL_DIR}\"; [ -n \"$d\" ] || d=\"${CLAUDE_PROJECT_DIR}/.claude/skills/octocode-awareness\"; s=\"$d/scripts/hooks/stop-verify.sh\"; [ -x \"$s\" ] && exec \"$s\" || exit 0'", timeout: 20 }] }]
  SessionEnd: [{ hooks: [{ type: command, command: "sh -c 'd=\"${CLAUDE_SKILL_DIR}\"; [ -n \"$d\" ] || d=\"${CLAUDE_PROJECT_DIR}/.claude/skills/octocode-awareness\"; s=\"$d/scripts/hooks/session-end.sh\"; [ -x \"$s\" ] && exec \"$s\" || exit 0'", timeout: 20 }] }]
  UserPromptSubmit: [{ hooks: [{ type: command, command: "sh -c 'd=\"${CLAUDE_SKILL_DIR}\"; [ -n \"$d\" ] || d=\"${CLAUDE_PROJECT_DIR}/.claude/skills/octocode-awareness\"; s=\"$d/scripts/hooks/notify-deliver.sh\"; [ -x \"$s\" ] && exec \"$s\" || exit 0'", timeout: 20 }] }]
---

# Octocode Awareness
Local SQLite-backed memory, file locks, notifications, and verify-before-conclude for coding agents. One shared store: `~/.octocode/memory/awareness.sqlite3`; scope by columns (`--workspace`, repo/ref), not per-repo DBs.

## Loop
1. Attend: run `status`, `get-memory`, `refine-get`, and `notify-get`; validate recalled code facts against current files.
2. Focus and claim: before writes, call `pre-flight-intent` with absolute `--target-file`; if exit `2`, stop or wait with `wait-for-lock`.
3. Verify: run the declared `--test-plan`, then `verify --workspace "$PWD" --all-pending` or `release-file-lock --verified`.
4. Encode: memories are concise reusable lessons with a reason (global by default, optionally workspace/repo/ref scoped); refinements are repo handoffs; notifications are live repo messages; durable repo guidance belongs in `AGENTS.md`/`CLAUDE.md` and must be reported to the user.
5. Sleep: release locks on failure too, prune stale data deliberately, and use `reflect --task ... --outcome ...` for lessons or reasoned harness proposals; ask before applying AGENTS/docs/standing-memory/skill-code harness changes.

## References
- `references/brain-model.md` — when tuning recall, cleanup, salience, corpus notes, or sleep behavior.
- `references/memory-recall.md` — when recording, recalling, labeling, superseding, or semantically indexing memories.
- `references/learning-capture.md` — when a research/brainstorm/investigation produces a durable insight: store it with `--reference` sources so the next agent recalls the conclusion and its provenance instead of re-researching.
- `references/coordination-protocol.md` — when writing lock, wait, release, refinement, or notification payloads.
- `references/files-awareness.md` — when a dirty repo or concurrent agents create collision risk.
- `references/self-harness.md` — when using verify gates, weakness mining, reflection, or harness refinements.
- `references/corpus.md` — when maintaining curated `~/.octocode/awareness/corpus/**/*.md` notes.
- `references/hooks.md` — before installing, auditing, tuning, or removing automatic hooks.
- `references/harness-apply.md` — when a human approves editing this skill or harness.
- `references/agentic-flows.md` — when composing lifecycle hooks, handoffs, subagents, and cleanup.
- `references/data-view.md` — whenever the user asks to show, view, browse, or prune awareness data.
- `references/show-memories.md` — when auditing older viewer behavior; `data-view.md` is canonical.
- `references/octocode.md` — when choosing Octocode MCP vs CLI for code research.
- `references/similar-systems.md` — when comparing or redesigning agent-memory systems.

## Scripts
- `scripts/awareness.py` — main CLI for memory, locks, verification, notifications, and refinements.
- `scripts/install.mjs` — check runtime dependencies and run schema plus awareness smoke tests.
- `scripts/install-hooks.mjs` — install or check always-on project/global PreToolUse and PostToolUse hooks.
- `scripts/schema.mjs` — validate wrapper payload JSON and print schema examples.
- `scripts/show-memories.py` — render the HTML viewer for memories, locks, intents, notifications, and refinements.
- `scripts/prune-stale-locks.sh` — cron-friendly stale lock cleanup wrapper.
- `scripts/smoke-multi-agent.mjs` — smoke-test claim, conflict, wait, notify, release, and stale-prune flows.

## Install
Run `node <skill_root>/scripts/install.mjs --check-only` for readiness. Always-on file-lock enforcement is separate: use `node <skill_root>/scripts/install-hooks.mjs --check --global` for user-scope Claude settings or `--project-dir <repo>` for repo scope; preview writes with `--dry-run` unless the user already approved the install. Installed settings hooks cover only PreToolUse/PostToolUse file locks; Stop/SessionEnd/UserPromptSubmit remain skill-scoped.
