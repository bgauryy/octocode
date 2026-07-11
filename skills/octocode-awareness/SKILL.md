---
name: octocode-awareness
description: "Use when planning, editing, reviewing, testing, or handing off work in a shared repo — even solo across sessions. Covers multi-agent coordination, collision avoidance, verification debt, durable memory/wiki, hooks setup/debug, and repo learning."
hooks:
  PreToolUse: [{ matcher: "^(?:Write|Edit|MultiEdit|NotebookEdit)$", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/pre-edit.sh", timeout: 20 }] }]
  PostToolUse: [{ matcher: "^(?:Write|Edit|MultiEdit|NotebookEdit)$", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/post-edit.sh", timeout: 20 }] }]
  PostToolUseFailure: [{ matcher: "^(?:Write|Edit|MultiEdit|NotebookEdit)$", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/post-edit.sh", timeout: 20 }] }]
  SubagentStart: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/notify-deliver.sh", timeout: 20 }] }]
  Stop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SubagentStop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  PreCompact: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/session-compact.sh", timeout: 20 }] }]
  SessionEnd: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/session-end.sh", timeout: 20 }] }]
  UserPromptSubmit: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/notify-deliver.sh", timeout: 20 }] }]
---
# Octocode Awareness
AGENTS routes; skill decides; CLI/SQLite acts; hooks automate deterministic edges.
This skill supplies operating policy; run every live-state action through the CLI. Treat memory and `.octocode/` wiki files as bounded leads, never authority.
## Lifecycle
Choose `<cli>`: monorepo `node packages/octocode-awareness/out/octocode-awareness.js`; installed `npx @octocodeai/octocode-awareness`. Export one stable `OCTOCODE_AGENT_ID`.
**BEFORE/READ+REASON -> DURING/DO -> AFTER/VERIFY -> LEARN? -> CLEAN? -> PROJECT?**
1. **BEFORE/READ+REASON:** run `attend`; follow `next`; state goal, acceptance, affected scope, and evidence. Use `memory recall --smart` only when prior learning could change the plan; choose a task or WORK.
2. **DURING/DO:** declare every edited path through hooks or `work start`; read interacting peer context. Ordinary overlap is allowed. Lock only sensitive paths; never bypass a conflict.
Hooks never choose plans, locks, success, learning, cleanup, or projection.
3. **AFTER/VERIFY:** run the declared check while present; `task submit` or `work end`; immediately `verify mark`, then `verify audit`. Ending, release, or expiry never means success.
4. **OPTIONAL:** `reflect record --lesson` only for verified reusable outcomes; clean only under pressure; project only when file readers need refreshed leads.
Example: `attend -> work start -> edit/test -> work end -> verify mark -> verify audit`; exclusive work uses `lock acquire -> lock release --status PENDING -> verify`, and stale removal starts with `lock prune --expired-only --dry-run`.
## Feature map — all features; load owners only for depth
- **Orient/state:** `attend`, `workspace status`, `query*`, `session capture`, and agent registry expose live work and continuity. Load `references/architecture.md` when inspecting storage or sessions.
- **Plan/task:** plans hold goals/docs; tasks handle dependencies, readiness, claim, heartbeat, submit, and release. Load `references/plan-task-workflow.md` when choosing a plan, task, or WORK.
- **Work/files:** WORK `start/touch/end/list/show` declares advisory files; overlaps coordinate. Load `references/files-awareness.md` when peer presence or overlap matters.
- **Locks/verify:** locks `acquire/wait/release/prune`; verification `mark/audit` separates completion from proof. Load `references/lock-protocol.md` for exclusive locks or verify details.
- **Signals/refinements:** signals `list/publish/reply/ack/resolve/prune`; refinements `get/set/delete` share query context. Load `references/coordination-protocol.md` when peers interact.
- **Memory:** `recall/record/forget/archive/restore` supports filters, ranking, and lifecycle. Load `references/memory-recall.md` when recalling or recording memory.
- **Reflection/review:** `reflect record/mine-weakness/developer-review/export-harness` turns verified outcomes into gated improvements. Load `references/improve-loop.md` when improving the harness.
- **Knowledge/projection:** docs `list/show/staleness`; `repo inject` refreshes `.octocode/` leads. Never hand-edit `.octocode/`. Load `references/output-routing.md` when choosing output.
- **Hooks/hosts:** hooks `install/check/remove` and `hook run` automate lifecycle edges across Claude, Codex, Cursor, and Pi. Load `references/hooks.md` when debugging automation.
- **Maintenance/contracts:** maintenance `init/digest/self-test`; schema `commands/list/path/json-schema/example/validate` owns machine contracts. Load `references/bookkeeping.md` when deciding whether to learn or clean.
- **Recipes:** when a start, finish, or command recipe is unknown, load `references/agent-cheatsheet.md`; unknown owner only: `docs list --compact`, then `docs show <name>`.
- **Skill evolution:** when shipping a skill change, load `references/skill-evolution.md`; use `octocode-eval` for goal/KPI, `octocode-research` for evidence, and `octocode-skills` for review.
Scripts: `scripts/awareness.mjs` fallback; `scripts/schema.mjs` contracts; `scripts/hook-runner.mjs` events; `scripts/extract-hook-files.mjs` paths; `scripts/install.mjs` diagnosis. Run `scripts/smoke-multi-agent.mjs` when validating coordination.
## Installation and first activation
When installing or activating, load `README.md`; initialize once, then `attend` in each repo.
Claude uses this frontmatter; do not install duplicate project hooks. When configuring Codex/Cursor, load `references/hooks.md`; Pi uses its bridge. Manual CLI remains valid. Developers rebuild with `yarn workspace @octocodeai/octocode-awareness build`; never hand-edit mirrors.
