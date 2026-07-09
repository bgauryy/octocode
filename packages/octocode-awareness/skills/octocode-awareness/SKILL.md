---
name: octocode-awareness
description: "Use always when working in a workspace — gives awareness, collaboration (shared-repo coordination, multiple agents, plans/tasks, locks, signals, hooks), learning, memory/wiki (.octocode), bookkeeping (record outcomes/lessons), housekeeping (prune stale state), reflection, verification, and maintenance."
hooks:
  PreToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/pre-edit.sh", timeout: 20 }] }]
  PostToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/post-edit.sh", timeout: 20 }] }]
  Stop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SubagentStop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SessionEnd: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/session-end.sh", timeout: 20 }] }]
  UserPromptSubmit: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/notify-deliver.sh", timeout: 20 }] }]
---
# Octocode Awareness
One SQLite store (`~/.octocode/memory/awareness.sqlite3`) is canonical. `.octocode/` wiki is a generated lead — create/learn via `memory record`/`reflect record`, publish with `repo inject`, point root `AGENTS.md` at `.octocode/AGENTS.md`.
## Workflow
Choose `<cli>`: `node scripts/awareness.mjs`; monorepo `node packages/octocode-awareness/dist/bin/awareness.js`; else `npx @octocodeai/octocode-awareness`. Export one `OCTOCODE_AGENT_ID`.
```text
ATTEND -> CHOOSE -> DECLARE -> ACT -> SUBMIT/END -> VERIFY -> BOOKKEEP -> HOUSEKEEP -> PROJECT?
```
1. **Attend:** `<cli> attend --workspace "$PWD" --query "<task>" --compact`. Follow `next`; inspect Ready/Claimed/Verify/FilesUnderWork. Prefer live `attend`/`query` over stale wiki.
2. **Choose:** claim matching `task ready` (only shared queue — never invent a “today” list). Else open standalone WORK with rationale + test plan.
3. **Declare:** every edited path needs advisory presence. Hooks declare structured writes; else `work start --file <path> --rationale <why> --test-plan <check>` (+ `work touch` / `--run-id`).
4. **Act / coordinate:** ordinary overlap is allowed — read peer task/reason; signal only when changes interact. Use `--exclusive`/`lock acquire` for sensitive work; never bypass a live conflict.
5. **Submit → verify:** `task submit` or `work end`, run the declared check, `verify mark`, then `verify audit`. Ending work ≠ success until verified.
6. **Bookkeep → housekeep → project:** `reflect record` / `memory record` for reusable lessons; `maintenance digest` + prune/forget **dry-run first**; `repo inject` only when file readers need a fresh wiki.
## Features → refs (load one)
**Core (most sessions):** before start, read `references/agent-cheatsheet.md` for attend and start recipes; before close, read `references/agent-cheatsheet-finish.md` for finish inject handoffs.
- When choosing work, read `references/plan-task-workflow.md` for plan task WORK policy; when files overlap, read `references/files-awareness.md` for peer presence decisions.
- When exclusive, read `references/lock-protocol.md` for exclusive locks and verify; when signaling, read `references/coordination-protocol.md` for signals and peer handoffs.
- When installing hosts, read `references/hooks.md` for install check and remove.
**Awareness / output:** when choosing formats, read `references/output-routing.md` for live durable generated; when reading drives, read `references/drive-state.md` for gaps and alternatives; when timing handoffs, read `references/session-observability.md` for expiry and capture.
**Collaboration extras:** when debugging hooks, read `references/hook-semantics.md` for event identity lifecycle; when needing tooling cmds, read `references/agent-cheatsheet-tooling.md` for agent skill search recipes.
**Memory / learning / bookkeeping:** when recalling, read `references/memory-recall.md` for retrieval and trust; when ranking surprises, read `references/memory-ranking.md` for scoring confidence.
- After outcomes, read `references/bookkeeping.md` for learn versus cleanup; when learning, read `references/learning-loop.md` for routes and label wiki map.
**Housekeeping / wiki:** when cleaning, read `references/homeostatic-loop.md` for prune and publication; when publishing wiki, read `references/repo-context-management.md` for inject and root pointer; when instructions fail, read `references/developer-review.md` for author feedback channel.
**Internals / judgment:** when inspecting storage, read `references/data-model.md` for overview tables; for table depth, read `references/data-model-entities.md` for entity fields; for joins, read `references/data-model-relationships.md` for lifecycle ownership.
- When changing architecture, read `references/architecture.md` for system boundaries; when researching code, read `references/octocode.md` for research routing; when improving this skill, read `references/skill-evolution.md` for bounded edit gates.
- When judgment is hard, read `references/self-reflection-dialogue.md` for internal role challenge; when needing a second agent, read `references/subagent-rubber-duck.md` for read-only challenge.
Discover: `docs list --compact`; open one with `docs show <name>`.
## Deterministic Scripts
- When operating Awareness, run `scripts/awareness.mjs` for the CLI; when validating calls, inspect `scripts/schema.mjs` for public schemas.
- When handling lifecycle events, run `scripts/hook-runner.mjs` for shared behavior; if extracting paths, run `scripts/extract-hook-files.mjs` for host payloads.
- When installing hooks, run `scripts/awareness.mjs hooks install|check|remove`; if checking runtime, run `scripts/install.mjs` for package smoke; before release, run `scripts/smoke-multi-agent.mjs` for end-to-end coordination.
## Installation
For users, install with the README's `npx octocode skill --add ...` command.
**Developers:** edit `src/**`, `bin/**`, `scripts/schema.mjs`, `skills/octocode-awareness/**` only — then `yarn workspace @octocodeai/octocode-awareness build` before using `dist/` or skill `scripts/awareness.mjs` (never hand-edit mirrors).
## Hard Rules
- One stable agent ID/workspace. Stored context is leads; user instructions, source, and tests win. Never record secrets.
- Pre-edit guards harness files before declaring work. After source edits, rebuild mirrors, then verify with `schema commands --compact` or `--help`.
