---
name: octocode-awareness
description: "Use when planning, editing, reviewing, testing, or handing off work in a shared repo — even solo across sessions. Covers multi-agent coordination, collision avoidance, verification debt, durable memory/wiki, hooks setup/debug, and repo learning."
hooks:
  PreToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/pre-edit.sh", timeout: 20 }] }]
  PostToolUse: [{ matcher: "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/post-edit.sh", timeout: 20 }] }]
  Stop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SubagentStop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SessionEnd: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/session-end.sh", timeout: 20 }] }]
  UserPromptSubmit: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/notify-deliver.sh", timeout: 20 }] }]
---
# Octocode Awareness
Coordinate repository work through canonical SQLite state; treat memory and `.octocode/` wiki output as bounded leads, never authority.
## Loop
Choose `<cli>`: `npx @octocodeai/octocode-awareness` (or global `octocode-awareness`); local monorepo build `node packages/octocode-awareness/dist/bin/awareness.js`; bundled `node scripts/awareness.mjs` only as fallback. Export one `OCTOCODE_AGENT_ID`; install host hooks so edits declare presence.
```text
BEFORE/READ+REASON -> DURING/DO -> AFTER/VERIFY -> LEARN? -> CLEAN? -> PROJECT?
```
1. **BEFORE/READ+REASON:** `<cli> attend --workspace "$PWD" --query "<task>" --agent-id "$OCTOCODE_AGENT_ID" --compact`; follow `next`; prefer live state to wiki. State goal, acceptance, affected scope, and evidence; claim `task ready` or open WORK.
2. **DURING/DO:** declare every edited path via hooks or `work start`; read peer reason before interacting changes. Ordinary overlap is allowed; exclusivity only for sensitive paths — never bypass a conflict.
3. **AFTER/VERIFY:** `task submit` or `work end`, run the declared check, `verify mark`, then `verify audit`. Ending work is not verified success.
4. **LEARN / CLEAN / PROJECT only when due:** record reusable outcomes; cleanup dry-run under workboard pressure; `repo inject` only when file readers need refreshed leads.
## Routes (load one owner; core work needs none)
- When a start, finish, or command recipe is unknown, load `references/agent-cheatsheet.md` for the executable path.
- When choosing a plan, task, or WORK, load `references/plan-task-workflow.md` for queue policy.
- When the same path has peer presence or overlap, load `references/files-awareness.md` for overlap protocol.
- When exclusive locks or verify mark/audit are needed, load `references/lock-protocol.md` for exclusive and verify recipes.
- When peers, signals, or refinements interact, load `references/coordination-protocol.md` for coordination rules.
- When installing or debugging automation, load `references/hooks.md` for host-specific execution and health checks.
- When choosing live, durable, or generated output, load `references/output-routing.md` for the smallest consumer surface.
- When recalling or recording memory, load `references/memory-recall.md` for trust and ranking rules.
- When deciding whether to learn or clean, load `references/bookkeeping.md` for pressure-driven triggers.
- When inspecting storage or sessions, load `references/architecture.md` for data owners and invariants.
- When improving the harness, load `references/improve-loop.md` for goal/KPI acceptance.
- When shipping a skill change, load `references/skill-evolution.md` for held-out and human gates.
Unknown owner only: `docs list --compact`; then `docs show <name>`.
## Scripts
Prefer package CLI above; `scripts/awareness.mjs` is bundled fallback. Use `scripts/schema.mjs` for contracts, `scripts/hook-runner.mjs` for host events, `scripts/extract-hook-files.mjs` for payloads, `scripts/install.mjs` for setup diagnostics, and `scripts/smoke-multi-agent.mjs` for end-to-end coordination smoke.
## Install · Hard rules
Install: README `npx octocode skill --add --path "$(npm root --global)/@octocodeai/octocode-awareness/dist/skills/octocode-awareness" --platform <host> --force`. Devs edit sources then `yarn workspace @octocodeai/octocode-awareness build` — never hand-edit mirrors. Hard: one agent ID/workspace; secrets out; manual CLI fallback; config ≠ runtime health; rebuild after edits; hooks + stable ID required for multi-agent presence.
