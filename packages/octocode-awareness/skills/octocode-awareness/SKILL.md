---
name: octocode-awareness
description: "Use when shared repository state can change the next action: peers, plans, overlap, locks, messages, verification debt, handoffs, or reusable memory. Skip routine solo work without a shared-state signal."
hooks:
  PreToolUse: [{ matcher: "^(?:Write|Edit|MultiEdit|NotebookEdit)$", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/pre-edit.sh", timeout: 20 }] }]
  PostToolUse: [{ matcher: "^(?:Write|Edit|MultiEdit|NotebookEdit)$", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/post-edit.sh", timeout: 20 }] }]
  PostToolUseFailure: [{ matcher: "^(?:Write|Edit|MultiEdit|NotebookEdit)$", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/post-edit.sh", timeout: 20 }] }]
  SubagentStart: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/notify-deliver.sh", timeout: 20 }] }]
  Stop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  SubagentStop: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/stop-verify.sh", timeout: 20 }] }]
  UserPromptSubmit: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/notify-deliver.sh", timeout: 20 }] }]
  Notification: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/notify-deliver.sh", timeout: 20 }] }]
  PreCompact: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/session-compact.sh", timeout: 20 }] }]
  PostCompact: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/session-compact.sh", timeout: 20 }] }]
  SessionEnd: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/session-end.sh", timeout: 20 }] }]
---
# Octocode Awareness

Flow: **NOTICE → SCOPE/IDENTITY → INSPECT → ACT → OBSERVE → SETTLE/VERIFY → LEARN**

CLI reports operational state and advice from observed records. Hooks guard edits and emit changed pointers; the host owns context, tools, budgets, and workers.

Public runner: `npx @octocodeai/octocode-awareness`; it bundles this skill. Install: `npx @octocodeai/octocode-awareness skill install --platform shared --project-dir "$PWD" --dry-run`; preview, then ask before rerun. `skill install --help` lists scopes; drift needs `--force`. `npx @octocodeai/octocode-awareness docs list --compact` reads bundled refs.

## Start small

```bash
export OCTOCODE_AGENT_ID="${OCTOCODE_AGENT_ID:-agent}"
npx @octocodeai/octocode-awareness attend --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --compact
npx @octocodeai/octocode-awareness work start --agent-id "$OCTOCODE_AGENT_ID" --file <path> --rationale "<scope>" --test-plan "<check>" --compact
```

End/submit, check, `verify mark`, then `verify audit`. Default durable Awareness state is `$OCTOCODE_HOME/awareness/awareness.sqlite3`; repository scope uses `<workspace>/.octocode/awareness.sqlite3`; `--db <path>` wins. It never uses the Agent control or runtime databases.

## Operational rules

1. **NOTICE** — Attend after a meaningful shared-state signal; read `next` first. When unchanged, continue authorized work, not recursive attending.
2. **SCOPE/IDENTITY** — Keep one stable ID and store; workspace selects isolation. Inspect the owner.
3. **INSPECT** — Read the relevant owner; overlap is advisory. `plan`/`task`/`work`/`lock`/`verify`/`memory`/`agent`/`signal` own the ledger; use `verify`/`signal`, not `check`/`message`.
4. **ACT** — Declare bounded work: paths/check. Signals and memory advise; locks, schemas, and verification debt enforce. Use an exclusive lock only for unsafe, non-mergeable state; record observed receipts.
5. **OBSERVE** — Run the declared check. Search hits, expiry, memories, and peer notes are leads, not proof; measured output proves its stated fact.
6. **SETTLE/VERIFY** — End/submit creates debt; mark and audit it. Expiry or lock release recovers state, never completion or success.
7. **LEARN** — After verification, reflect reusable lessons, failures, or follow-up; leave a handoff only when continuation is real.

## Operational physiology

Use `operational_state` and `regulation` for bounded corrections, then remeasure. Unknown sensors stay unknown; never invent or infer them. Native context guidance needs a fresh matching limit and cannot compact. Advice neither authorizes action nor proves success. Cleanup remains dry-run-first.

Preview `hooks install --host <host> --profile <profile> --dry-run`, then ask before applying. Pi uses native events.

## Capability map

`schema commands --compact` lists routes; `schema command <noun> [action]` shows flags.

| Need | Live routes |
|---|---|
| Orient | `attend`, `status`, `query`, `docs`, `schema` |
| Plan/execute | `plan`, `task`, `work`, `lock`, `verify` |
| Coordinate | `agent`, `signal`, `handoff`, `session capture` |
| Learn/follow up | `memory`, `refinement`, `reflect` |
| Operate/maintain | `maintenance`, `database`, `config`, `hooks`, `hook run` |

Use `schema entities --compact`; `--all` adds kind/owner.

## Load detail only when needed

- When storage/ownership matters, load [configuration](references/configuration.md), [architecture](references/architecture.md), [data model](references/data-model.md), [agent cheat sheet](references/agent-cheatsheet.md).
- When sharing work, load [flow matrix](references/flow-matrix.md), [protocol](references/coordination-protocol.md), [plans/tasks](references/plan-task-workflow.md), [files](references/files-awareness.md), [locks](references/lock-protocol.md).
- For learning, load [memory](references/memory-recall.md), [learning loop](references/learning-loop.md), [reflection](references/self-reflection-dialogue.md), [homeostatic loop](references/homeostatic-loop.md) for pressure.
- For runtime, load [hooks](references/hooks.md), [output routing](references/output-routing.md), [research](references/octocode.md), [config schema](references/awareness-config.schema.json).
- When a host runtime needs embedded entry points, use `scripts/awareness.mjs` (CLI), `scripts/install.mjs` (diagnostic), `scripts/extract-hook-files.mjs` (extract), `scripts/hook-runner.mjs` (dispatch), `scripts/smoke-multi-agent.mjs` (isolated smoke), and `scripts/hooks/*.sh` (host events).

Edit this source; `yarn workspace @octocodeai/octocode-awareness build` refreshes package mirrors.
