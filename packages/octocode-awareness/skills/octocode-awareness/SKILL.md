---
name: octocode-awareness
description: "Use when an agent plans, edits, reviews, tests, or hands off work in a code repository—even solo across sessions. Run the Homeostatic Awareness Loop over shared SQLite state and optional hooks: attend, choose a plan/task or standalone work, declare advisory file presence, coordinate overlaps/signals, lock sensitive paths, verify outcomes, preserve reusable memory/reflection, maintain state, and publish a bounded `.octocode` wiki. Also use for multi-agent collaboration, continuity, Awareness setup/debugging, hook conflicts, verification debt, or repo learning."
hooks:
  PreToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/pre-edit.sh", timeout: 20 }] }]
  PostToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/post-edit.sh", timeout: 20 }] }]
  Stop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SubagentStop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SessionEnd: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/session-end.sh", timeout: 20 }] }]
  UserPromptSubmit: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/notify-deliver.sh", timeout: 20 }] }]
---
# Octocode Awareness
Treat the workspace as a living shared organ only as an operational metaphor: SQLite is canonical; `.octocode/` holds plan docs and bounded leads; evidence and human gates control change.
## Workflow
Choose `<cli>`: `node scripts/awareness.mjs`; monorepo `node packages/octocode-awareness/dist/bin/awareness.js`; else `npx @octocodeai/octocode-awareness`. Export one `OCTOCODE_AGENT_ID`.
```text
ATTEND -> CHOOSE -> DECLARE -> ACT -> SUBMIT/END -> VERIFY -> BOOKKEEP? -> HOUSEKEEP? -> PROJECT?
```
1. **Attend:** `<cli> attend --workspace "$PWD" --query "<task>" --compact`. Follow `next`; inspect Ready/Claimed/Verify/FilesUnderWork. Prefer live `attend`/`query` over stale wiki.
2. **Choose:** claim matching `task ready` (only shared queue — never invent a “today” list). Else open standalone WORK with rationale + test plan.
3. **Declare:** every edited path needs advisory presence. Hooks declare structured writes; else `work start --file <path> --rationale <why> --test-plan <check>` (+ `work touch` / `--run-id`).
4. **Act / coordinate:** ordinary overlap is allowed — read peer task/reason; signal only when changes interact. Use `--exclusive`/`lock acquire` for sensitive work; never bypass a live conflict.
5. **Submit → verify:** `task submit` or `work end`, run the declared check, `verify mark`, then `verify audit`. Ending work ≠ success until verified.
6. **Conditional Bookkeeping/Housekeeping:** record only reusable lessons; digest/prune (**dry-run first**) only under cleanup pressure; `repo inject` only when file readers need a fresh wiki.
## Next skills
- `octocode-research` — evidence before/while ACT (local + GitHub/npm)
- `octocode-brainstorming` → `octocode-rfc-generator` — idea → Build RFC → plan packet
- `octocode-roast` — blunt review pass; `octocode-skills` / `octocode-prompt-optimizer` — evolve skills/harness docs

## Features → refs (load one owner; Core (most sessions) needs none)
- When a recipe, start, finish, or install path is unknown, load `references/agent-cheatsheet.md` for command and deeper routing.
- When choosing plans, tasks, or standalone Work, load `references/plan-task-workflow.md` for queue and execution policy.
- When coordinating peers or follow-up, load `references/coordination-protocol.md` for signals, refinements, files, and locks.
- When configuring host automation, load `references/hooks.md` for installation and event semantics.
- When choosing live, durable, or generated output, load `references/output-routing.md` for the smallest consumer surface.
- When using memory, load `references/memory-recall.md` for retrieval, trust, and ranking routes.
- When learning or cleaning, load `references/bookkeeping.md` for bookkeeping and housekeeping triggers.
- When reasoning about living-system pressure, load `references/homeostatic-loop.md` for sensors, actuators, guards, and remeasurement.
- When inspecting architecture, storage, sessions, or drive fields, load `references/architecture.md` for owners and context invariants.
- When evolving skills or the harness, load `references/skill-evolution.md` for human gates and held-out acceptance.
Unknown owner only: `docs list --compact`; then `docs show <name>`.
## Scripts
- When operating or validating, run `scripts/awareness.mjs` for CLI transitions and `scripts/schema.mjs` for public contracts.
- When handling host events, run `scripts/hook-runner.mjs` for lifecycle behavior.
- When extracting host payload paths, run `scripts/extract-hook-files.mjs` for normalized targets.
- When installing hooks, run `scripts/install.mjs` for runtime and setup guidance.
- Before release, run `scripts/smoke-multi-agent.mjs` for end-to-end coordination verification.
## Installation
Users install with the README's `npx octocode skill --add ...`. Developers edit sources only, then run `yarn workspace @octocodeai/octocode-awareness build`; never hand-edit mirrors.
## Hard Rules
- One stable agent ID/workspace. Stored context is leads; user instructions, source, and tests win. Never record secrets.
- Pre-edit guards harness files before declaring work. After source edits, rebuild mirrors, then verify with `schema commands --compact` or `--help`.
