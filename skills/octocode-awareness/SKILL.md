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
The Octocode Awareness Agent Skill ships with the `@octocodeai/octocode-awareness` CLI package: use `npx @octocodeai/octocode-awareness` when installed, or the local package build in this monorepo.
## Loop
Choose `<cli>`: `npx @octocodeai/octocode-awareness` (or global `octocode-awareness`); local monorepo build `node packages/octocode-awareness/dist/bin/awareness.js`; bundled `node scripts/awareness.mjs` only as fallback. Export one `OCTOCODE_AGENT_ID`; use First activation below for setup and host wiring.
```text
BEFORE/READ+REASON -> DURING/DO -> AFTER/VERIFY -> LEARN? -> CLEAN? -> PROJECT?
```
1. **BEFORE/READ+REASON:** `<cli> attend --workspace "$PWD" --query "<task>" --agent-id "$OCTOCODE_AGENT_ID" --compact`; follow `next`; prefer live state to wiki. State goal, acceptance, affected scope, and evidence; run `task ready`, claim a returned ID, or open WORK.
2. **DURING/DO:** declare every edited path via hooks or `work start`; read peer reason before interacting changes. Ordinary overlap is allowed; exclusivity only for sensitive paths — never bypass a conflict.
3. **AFTER/VERIFY:** run the declared check while presence remains active; `task submit` or `work end`, immediately `verify mark`, then `verify audit`. Ending work is not verified success.
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
- When improving the harness, prefer `octocode-eval`; else load `references/improve-loop.md` — minimum goal/KPI accept stub.
- When shipping a skill change, load `references/skill-evolution.md` for held-out and human gates.
Unknown owner only: `docs list --compact`; then `docs show <name>`.
## Related skills
- `octocode-eval` — goal→KPI when measuring harness changes
- `octocode-research` — code/GitHub evidence (not Awareness search)
- `octocode-skills` — skill folder create/edit/review (bundled sibling)
- `octocode-subagent` — parallel writers in this repo
- `octocode-rfc-generator` — design before large coordination changes
## Scripts
Prefer package CLI above; `scripts/awareness.mjs` is bundled fallback. Use `scripts/schema.mjs` for contracts, `scripts/hook-runner.mjs` for host events, `scripts/extract-hook-files.mjs` for payloads, `scripts/install.mjs` for setup diagnostics, and `scripts/smoke-multi-agent.mjs` for end-to-end coordination smoke.
## Installation and first activation
Package/skill install and refresh live in `README.md`. After activation: export one stable `OCTOCODE_AGENT_ID`; run `scripts/install.mjs` when the bundled runtime needs diagnosis; run `<cli> maintenance init --compact` once; then `attend` in each repo.
Claude skill frontmatter is already one hook surface — do not also install duplicate project settings. Codex/Cursor load `references/hooks.md` for preview → approval → install → strict check; Pi uses its bridge only.
Developers edit this repo-root skill and CLI/runtime sources under `packages/octocode-awareness`; rebuild with `yarn workspace @octocodeai/octocode-awareness build`. Never hand-edit mirrors. Hard: one agent ID/workspace; secrets out; manual fallback; config ≠ runtime health; hooks + stable ID for multi-agent presence.
