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

The public runner for every agent-facing command is `npx @octocodeai/octocode-awareness`. The package bundles this skill, and `npx @octocodeai/octocode-awareness docs list --compact` discovers the bundled references (`docs show <name>` reads one). `scripts/awareness.mjs` is the embedded skill/runtime copy, not the normal user entry point.

## Start small

```bash
export OCTOCODE_AGENT_ID="${OCTOCODE_AGENT_ID:-agent}"
npx @octocodeai/octocode-awareness attend --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --compact
npx @octocodeai/octocode-awareness work start --agent-id "$OCTOCODE_AGENT_ID" --file <path> --rationale "<scope>" --test-plan "<check>" --compact
```

End/submit, check, `verify mark`, then `verify audit`. Default durable Awareness state is `$OCTOCODE_HOME/awareness/awareness.sqlite3`; repository scope uses `<workspace>/.octocode/awareness.sqlite3`; `--db <path>` wins. It never uses the Agent control or runtime databases.

## Operational rules

1. **NOTICE** — Attend after a meaningful shared-state signal that can change action. `next` reads first: audit verification or inspect work, inbox, or task. When unchanged, continue authorized work, not recursive attending.
2. **SCOPE/IDENTITY** — Keep one stable ID and store; workspace selects isolation. Inspect the owner.
3. **INSPECT** — Read relevant plan, task, run, lock, signal, or memory; overlap is advisory. Root `plan`/`task`/`work`/`lock`/`verify`/`memory`/`agent`/`signal` own the ledger. `check` and `message` are not routes; use `verify` and `signal`.
4. **ACT** — Declare bounded work: paths and a check. Presence, signals, and memory advise; locks, schemas, and verification debt enforce. Use an exclusive lock only for unsafe, non-mergeable state; record observed receipts.
5. **OBSERVE** — Run the declared check. Search hits, expiry, memories, and peer notes are leads, not proof; measured output proves its stated fact.
6. **SETTLE/VERIFY** — End/submit creates debt; mark the observed result and audit it. Expiry or lock release recovers state, never completion or success.
7. **LEARN** — After verification, reflect reusable lessons, recurring failures, or owned follow-up; leave a handoff only when continuation is real. Reflection never authorizes edits or configuration changes.

## Operational physiology

Use `operational_state` and advisory `regulation` for bounded corrections; remeasure after material change. Unknown sensors stay unknown; never invent or infer them. Native context guidance needs a fresh host measurement and matching active-model limit; it cannot compact. Advice neither authorizes action nor proves success. Cleanup remains dry-run-first.

Preview `hooks install --host <host> --profile <profile> --dry-run`, show it, then ask before applying. Pi uses native events.

## Capability map

Discover flags: `schema commands --compact`; use `schema command <noun> [action]` for one route.

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
- Scripts: `scripts/awareness.mjs` (CLI), `scripts/install.mjs` (install), `scripts/extract-hook-files.mjs` (extract), `scripts/hook-runner.mjs` (dispatch), `scripts/smoke-multi-agent.mjs` (isolated smoke), and `scripts/hooks/*.sh` (host events).

Edit this source; `yarn workspace @octocodeai/octocode-awareness build` refreshes package mirrors.
