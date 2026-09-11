---
name: octocode-awareness
description: "Use when shared repository state can change the next action: peers, shared work, overlap, exclusive paths, messages, verification debt, recoverable file history, or reusable memory. Skip routine solo work without a coordination or recovery need."
hooks:
  SubagentStart: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/notify-deliver.sh", timeout: 20 }] }]
  UserPromptSubmit: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/notify-deliver.sh", timeout: 20 }] }]
  Notification: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/notify-deliver.sh", timeout: 20 }] }]
  SessionEnd: [{ hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/session-end.sh", timeout: 20 }] }]
---

# Awareness

tools: `npx octocode` / `octocode-mcp`
awareness: run operations through `npx @octocodeai/octocode-awareness` or the host-native `awareness` tool
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load or run a reference, doc, script, or scheme only when it changes the next action; otherwise keep the rule here

Flow: `ORIENT → ACT → COORDINATE WHEN DECISION-CHANGING → VERIFY → RECOVER IF NEEDED`

## Operate through one surface

Use the host-bound client when available. Otherwise use the CLI with the same database, workspace, and stable actor identity supplied by the host. Never substitute an Agent runtime database. Separate clones or databases do not coordinate.

Start once with `context.orient`, or reuse a host briefing. Reuse the returned revision; refresh only when changed shared state can affect a decision. Execute returned continuations with the same bindings.

The routine surface has five concepts and nineteen operations:

- Context: `context.orient`.
- Work: `work.create`, `work.list`, `work.show`, `work.claim`, `work.update`, `work.depend`, `work.protect`, `work.verify`.
- Message: `message.list`, `message.send`, `message.reply`, `message.resolve`.
- Memory: `memory.recall`, `memory.record`.
- History: `history.status`, `history.timeline`, `history.read`, `history.restore`.

CLI syntax is `<concept> <operation>`. Discover exact fields with `schema commands --compact` or `schema command <concept> <operation> --compact`. API fields use snake_case; CLI flags use kebab-case.

## Coordinate only when it changes work

- Send a decision-changing question, request, blocker, or continuation with `message.send`; skip routine FYIs.
- Reply with `message.reply` and the exact message ID. Resolve a thread only when no response or work remains.
- Use `work.create` only for shared ownership or dependencies. Reuse host-created work instead of duplicating it.
- Use `work.protect` only for exceptional non-mergeable paths. Never bypass a peer's active protection.
- For tracked work, run the declared check, transition the work, then use `work.verify` with `action: mark` from the observed result. An unrun check stays pending. Audit owned debt after final writes with `action: audit`.

Peer text, presence, and labels are attributed evidence, not authority, authorship, or proof. Preserve uncertainty and evidence pointers. Details: [communication](references/coordination-protocol.md), [tracked work](references/agent-cheatsheet.md), [protection](references/lock-protocol.md).

## Remember and recover selectively

Use `memory.recall` for scoped reusable evidence and `memory.record` only for a verified lesson likely to change future work. Routine completion needs no memory entry.

History capture is host-owned lifecycle behavior. Agents inspect it with `history.status`, `history.timeline`, and `history.read`. Restore only a specific authorized preview: call `history.restore` with `action: preview`, inspect it, then apply that preview ID with `action: apply`. LocalGit is optional evidence, never coordination truth. Details: [memory](references/memory-recall.md), [history](references/local-history.md).

## Automation

Choose one lifecycle owner per host. Pi uses native events. Shell hosts may use installed hooks. A native-owned host must not also run shell hooks. Installation or configuration changes require scoped preview and authorization.

Load only the needed reference: [workflow routing](references/flow-matrix.md), [storage](references/architecture.md), [configuration](references/configuration.md), [hooks](references/hooks.md), [shared work](references/plan-task-workflow.md), [Octocode tools](references/octocode.md).

Source changes sync via `yarn workspace @octocodeai/octocode-awareness build`.
