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

Local SQLite-backed memory, file locks, notifications, and verify-before-conclude for coding agents. One shared store at `~/.octocode/memory/awareness.sqlite3`; scope records by workspace, repo, or ref — not per-repo DBs.

## When to invoke

Invoke this skill when any of:
- You are about to write/edit a file (hooks do this automatically)
- You are starting work and need to recall prior lessons
- Another agent may be working in the same repo
- You are finishing work and need to hand off cleanly
- You want to record a durable lesson or flag a codebase fix

## Agent loop

1. **Attend** — run `status`, `get-memory`, `refine-get`, `notify-get`; validate recalled facts against current files before trusting them.
2. **Claim** — before writes, call `pre-flight-intent --target-file <abs-path>`; if exit `2`, stop or wait with `wait-for-lock`. Hooks do this automatically.
3. **Work** — edit under the lock. Hooks release the lock automatically on save.
4. **Verify** — run the declared `--test-plan`, then record the result with `verify --all-pending` or `release-file-lock --verified`.
5. **Encode** — memories = concise reusable lessons (global or scoped); refinements = repo-fix queue for the next agent; notifications = live repo messages.
6. **Sleep** — release locks even on failure; run `reflect --task ... --outcome ...`; prune stale data. Report durable guidance to the user for `AGENTS.md`.

## References

- `references/memory-recall.md` — recording, recalling, labeling, superseding, or semantically indexing memories.
- `references/learning-capture.md` — storing research conclusions with `--reference` sources so the next agent recalls the verdict, not re-researches.
- `references/coordination-protocol.md` — lock, wait, release, refinement, and notification payloads.
- `references/files-awareness.md` — dirty repo or concurrent agent collision risk.
- `references/hooks.md` — installing, auditing, tuning, or removing automatic hooks.
- `references/self-harness.md` — verify gates, weakness mining, reflection, and harness refinements.
- `references/brain-model.md` — tuning recall, cleanup, salience, and sleep behavior.
- `references/agentic-flows.md` — composing lifecycle hooks, handoffs, subagents, and cleanup.
- `references/corpus.md` — maintaining curated `~/.octocode/awareness/corpus/` notes.
- `references/harness-apply.md` — when a human approves editing this skill or harness.
- `references/data-view.md` — showing, viewing, or pruning awareness data on request.
- `references/octocode.md` — choosing Octocode MCP vs CLI for code research.
- `references/similar-systems.md` — comparing or redesigning agent-memory systems.

## Install

Always-on file-lock hooks (PreToolUse/PostToolUse) can be installed once per user or per project:

```bash
# Check what would be installed
node <skill_root>/scripts/install-hooks.mjs --dry-run --global

# Install for all projects (user scope)
node <skill_root>/scripts/install-hooks.mjs --global

# Install for one project
node <skill_root>/scripts/install-hooks.mjs --project-dir <repo>
```

Stop / SessionEnd / UserPromptSubmit hooks are skill-scoped and activate when the skill is loaded. Verify the runtime is ready:

```bash
node <skill_root>/scripts/install.mjs --check-only
```
