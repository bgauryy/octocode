---
name: octocode-awareness
description: "Use when coordinating shared repo work. Run the compact Awareness CLI for workspace status, memory recall, file locks, signals, verification, hooks, repo context, durable lessons, and handoffs."
hooks:
  PreToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/pre-edit.sh", timeout: 20 }, { type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/harness-guard.sh", timeout: 20 }] }]
  PostToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/post-edit.sh", timeout: 20 }] }]
  Stop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SubagentStop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SessionEnd: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/session-end.sh", timeout: 20 }] }]
  UserPromptSubmit: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/notify-deliver.sh", timeout: 20 }] }]
---
# Octocode Awareness
Use this as the single operational awareness skill. It owns memory, file locks, verification, signals, reflection, hooks, and workspace `.octocode/` repo context projections; it does not replace code search, tests, or project instructions.
Canonical store: global `~/.octocode/memory/awareness.sqlite3`, scoped by workspace, artifact/package/service, repo, and ref.
Do not confuse it with `<repo>/.octocode/`; `repo inject` generates that workspace projection.
CLI preference: use the bundled/local CLI when present (`node scripts/awareness.mjs` inside an installed skill, or `node packages/octocode-awareness/dist/bin/awareness.js` in this repo). Use `npx @octocodeai/octocode-awareness` only when no local CLI exists.
In-repo first command: `<local-awareness-cli> workspace status --workspace "$PWD" --compact`.
Run `schema commands --compact` once for discovery; use `<command> --help` or `schema json-schema <name> --compact` for contracts. Pi exposes equivalent methods.
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

If older prompts name `octocode-agent-communication` or `octocode-reflection`, load this skill for the actual workflow. Claude-style hosts may execute the frontmatter hooks; Codex, Cursor, and Pi require `references/hooks.md` host wiring.

## References
- `references/full-flow.md` — when a task asks for the full CLI/skill/hooks/repo-context/self-reflection flow or technical onboarding.
- `references/memory-recall.md` — before planning, recall durable lessons and validate them against current files.
- `references/coordination-protocol.md` — before locking, waiting, releasing, signaling, or managing refinements.
- `references/files-awareness.md` — before planning/editing in dirty or concurrently edited workspaces.
- `references/hooks.md` — before installing, auditing, tuning, or removing hooks.
- `references/repo-context-management.md` — before generating, refreshing, sharing, or relying on workspace `.octocode/` repo context projections.
- `references/data-model.md` — when checking SQLite schema, memory rows, tasks, locks, or signals.
- `references/octocode.md` — when code research is needed; delegates Octocode research rules to `octocode-research`.

## Installation
- `<local-awareness-cli>` — prefer `node scripts/awareness.mjs` from this bundled skill, or `node packages/octocode-awareness/dist/bin/awareness.js` in this repo; fall back to `npx @octocodeai/octocode-awareness` only when no local CLI exists.
- `npx octocode skill --add --path "{{path_to_skills_location}}/octocode-awareness" --platform common` — install this bundled skill from the agent-known local skill folder. Replace `{{path_to_skills_location}}` before running; if it already points at the `octocode-awareness` folder, pass it directly.
- `node scripts/awareness.mjs` — bundled standalone fallback with the same command surface.
- `node scripts/schema.mjs`, `node scripts/install.mjs --check-only`, `node scripts/smoke-multi-agent.mjs` — inspect payload contracts, safely check dependencies, and smoke-test locks/signals/verification. Running `install.mjs` without `--check-only` may install optional local script dependencies.
- `scripts/install-hooks.mjs`, `scripts/hook-runner.mjs`, `scripts/extract-hook-files.mjs` — hook install wrapper, lifecycle dispatcher, and write-path extractor.
Preview hook writes with `<local-awareness-cli> hooks install --host cursor --dry-run --project-dir <repo> --compact`, `--host codex`, or `--host claude`.
