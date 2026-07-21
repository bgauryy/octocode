---
name: octocode-awareness
description: "Use when coordinating work in a shared repo — multi-agent or solo across sessions: collision avoidance, handoffs, verification debt, durable memory/wiki, hooks setup/debug, and repo learning before planning, editing, reviewing, or testing."
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
AGENTS routes; skill decides; CLI/SQLite acts; hooks automate deterministic edges. Memory and `.octocode/` are leads. Never hand-edit `.octocode/`.
Run live-state actions through the CLI.
When the host supports delegation, ALWAYS batch routine deterministic Awareness CLI reads, writes, and maintenance into one phase.
Use the smallest capable low-cost agent (for example Haiku or Composer 2.5); supply decided scope, require `--compact`, and cap its receipt at 512 bytes.
The lead retains destructive approval, conflict handling, memory-truth judgment, and verification.
Run directly only when the host cannot delegate.
## Lifecycle
`<cli>`: local `node packages/octocode-awareness/out/octocode-awareness.js`; installed `npx @octocodeai/octocode-awareness`. Set `OCTOCODE_AGENT_ID` per agent/session.
**BEFORE/READ+REASON -> DURING/DO -> AFTER/VERIFY -> LEARN? -> CLEAN? -> PROJECT?**
1. **BEFORE:** `attend`; follow `next`; state goal, acceptance, affected scope, and evidence. Use `memory recall --smart` only if it may change the plan; choose task or WORK.
2. **DURING:** declare paths via hooks or `work start`; read peers. Ordinary overlap is allowed; never bypass conflict. Hooks never choose plans, locks, success, learning, cleanup, or projection.
3. **AFTER:** check while present; `task submit`/`work end`; `verify mark`; `verify audit`. Expiry never means success.
4. **OPTIONAL:** `reflect record --lesson` only for verified reusable outcomes; clean only under pressure; project only for file readers.
Loop: `attend -> work start -> edit/check -> work end -> verify mark -> verify audit`; add `--exclusive` when required. `lock wait/prune` are recovery.
## Feature map — all features; load owners only for depth
- **Orient/state:** `attend`, `workspace status`, query, `session capture`, agent registry. When inspecting storage/session, load `references/architecture.md` for storage boundaries.
- **Plan/task:** dependencies/readiness/claim/heartbeat/submit/release. When choosing plan, task, or WORK, load `references/plan-task-workflow.md` for task routing.
- **Work/files:** `start/touch/end/list/show`. When overlap matters, load `references/files-awareness.md` for peer semantics.
- **Exclusive work/verify:** `work start --exclusive`, lock recovery, mark/audit. When exclusivity or debt matters, load `references/lock-protocol.md` for conflict gates.
- **Signals/refinements:** signal publish/list/reply/ack/resolve/prune; refinement get/set/delete. When peers interact, load `references/coordination-protocol.md` for peer flow.
- **Memory:** recall/record/forget/archive/restore; correct with `--supersedes`. When using memory, load `references/memory-recall.md` for trust rules.
- **Reflection/review:** reflect/review/export harness. When improving, load `references/improve-loop.md` for acceptance gates.
- **Knowledge/wiki:** docs/staleness, `wiki sync`, projections. When choosing output, load `references/output-routing.md` for lean routing.
- **Hooks/hosts:** install/check/remove/run across Claude/Codex/Cursor/Pi. When configuring/debugging, load `references/hooks.md`.
- **Maintenance/contracts:** maintenance and schema commands/list/path/json-schema/example/validate. When cleaning/learning, load `references/bookkeeping.md` for pressure gates.
- **Recipes:** when a start, finish, or command recipe is unknown, load `references/agent-cheatsheet.md`; unknown owner only: `docs list --compact`, then `docs show <name>`.
- **Skill evolution:** when shipping a skill change, load `references/skill-evolution.md`; use `octocode-eval` for goal/KPI, `octocode-research` for evidence, and `octocode-skills` for review.
Scripts: use `scripts/awareness.mjs` for fallback, `scripts/schema.mjs` for contracts, `scripts/hook-runner.mjs` for events, `scripts/extract-hook-files.mjs` for paths, `scripts/install.mjs --compact` for diagnosis; run `scripts/smoke-multi-agent.mjs` when validating coordination.
## Installation and first activation
Load `README.md`; initialize once, then `attend`. Claude uses frontmatter; do not install duplicate project hooks. When configuring Codex/Cursor, load `references/hooks.md`; Pi uses its bridge. Rebuild with `yarn workspace @octocodeai/octocode-awareness build`; never edit mirrors.
