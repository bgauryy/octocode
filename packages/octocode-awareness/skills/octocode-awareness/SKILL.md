---
name: octocode-awareness
description: "Use always when working in a workspace — awareness and collaboration for shared-repo coordination across multiple agents: plans/tasks, advisory file work, sensitive locks, signals/hooks, verification, memory/wiki, reflection, learning/bookkeeping, cleanup/housekeeping, and maintenance."
hooks:
  PreToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/pre-edit.sh", timeout: 20 }] }]
  PostToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/post-edit.sh", timeout: 20 }] }]
  Stop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SubagentStop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SessionEnd: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/session-end.sh", timeout: 20 }] }]
  UserPromptSubmit: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/notify-deliver.sh", timeout: 20 }] }]
---
# Octocode Awareness
SQLite is canonical; `.octocode/` holds plan docs and generated leads.
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
6. **Bookkeeping → Housekeeping → project:** `reflect record` / `memory record` for reusable lessons; `maintenance digest` + prune/forget **dry-run first**; `repo inject` only when file readers need a fresh wiki.
## Features → refs (load one; Core (most sessions) needs none)
- When a recipe is unknown, load `references/agent-cheatsheet.md` for start commands; when closing, load `references/agent-cheatsheet-finish.md` for conditional finish; when configuring skills, load `references/agent-cheatsheet-tooling.md` for install/search recipes.
- When choosing work, load `references/plan-task-workflow.md` for task policy; when files overlap, load `references/files-awareness.md` for overlap decisions.
- When using exclusivity, load `references/lock-protocol.md` for locks/verify; when messaging peers, load `references/coordination-protocol.md` for signal and refinement flows.
- When installing hooks, load `references/hooks.md` for host setup; when debugging events, load `references/hook-semantics.md` for lifecycle identity; when timing handoffs, load `references/session-observability.md` for expiry and capture timing.
- When choosing output, load `references/output-routing.md` for surface routing; when interpreting drives, load `references/drive-state.md` for gaps and alternatives.
- When recalling memory, load `references/memory-recall.md` for retrieval/trust; when ranks surprise, load `references/memory-ranking.md` for scoring diagnosis.
- When learning/cleaning, load `references/bookkeeping.md` for route choice; when routing learning, load `references/learning-loop.md` for closure rules.
- When cleaning deeply, load `references/homeostatic-loop.md` for prune intuition; when publishing wiki, load `references/repo-context-management.md` for projection rules; when instructions fail, load `references/developer-review.md` for author feedback.
- When inspecting storage, load `references/data-model.md` for table map; for fields load `references/data-model-entities.md` for entity detail; for joins load `references/data-model-relationships.md` for lifecycle joins.
- When changing boundaries, load `references/architecture.md` for system owners; when researching code, load `references/octocode.md` for evidence routing.
- When evolving this skill, load `references/skill-evolution.md` for edit gates; for hard judgment load `references/self-reflection-dialogue.md` for role challenge; for independent challenge load `references/subagent-rubber-duck.md` for duck review.
Unknown owner only: `docs list --compact`; then `docs show <name>`.
## Scripts
- When operating/validating, run `scripts/awareness.mjs` CLI and `scripts/schema.mjs` contracts.
- When handling host events/paths, run `scripts/hook-runner.mjs` for lifecycle behavior and `scripts/extract-hook-files.mjs` for payload path extraction.
- When installing/smoking/releasing, run `scripts/install.mjs`, `scripts/awareness.mjs hooks install|check|remove`, and `scripts/smoke-multi-agent.mjs` for end-to-end release verification.
## Installation
For users, install with the README's `npx octocode skill --add ...` command.
**Developers:** edit `src/**`, `bin/**`, `scripts/schema.mjs`, `skills/octocode-awareness/**` only — then `yarn workspace @octocodeai/octocode-awareness build` before using `dist/` or skill `scripts/awareness.mjs` (never hand-edit mirrors).
## Hard Rules
- One stable agent ID/workspace. Stored context is leads; user instructions, source, and tests win. Never record secrets.
- Pre-edit guards harness files before declaring work. After source edits, rebuild mirrors, then verify with `schema commands --compact` or `--help`.
