---
name: octocode-awareness
description: "Use when context pressure, repeated attempts, progress, shared work, peer activity, verification debt, recovery, or reusable memory can change an agent's next action. Self-monitoring applies during solo work; coordination is conditional."
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

Flow: `OBSERVE → ORIENT → ACT → FEEDBACK`; coordinate and recover when relevant.

## Operate through one surface

Use the host-bound client when available. Otherwise use the CLI with the same database, workspace, and stable actor/session identity supplied by the host. Never substitute an Agent runtime database. Separate clones or databases do not coordinate.

Keep the external kernel standing, then load only the canonical section needed for the next action. Import `getAwarenessAgentInstructions({ sections: ['coordination'] })`, replacing the section name as needed, or run `npx -y @octocodeai/octocode-awareness instructions --section <name>`. Sections are `start`, `observe`, `advise`, `feedback`, `coordination`, `trust`, and `schema`; reuse sections already supplied by the host.

Start once with `context.orient`, or reuse a host briefing. Retain its revision and refresh only when changed observations or shared state can affect a decision. Execute returned continuations with the same bindings. For an unfamiliar operation, discover exact fields with `schema commands --compact` or `schema command <concept> <operation> --compact`; Pi uses `describe:true`. Every descriptor exposes `inputSchemaText`, generated from its canonical schema rather than a hand-maintained parameter list. Load one operation schema when needed and reuse it; do not preload all schemas into standing context.

## Coordinate only when it changes work

- Send a decision-changing question, request, blocker, or continuation with `message.send`; skip routine FYIs. Reply with `message.reply` and the exact message ID; resolve when no response or work remains. Every kind expires and pruning waits for a grace window, so put durable lessons in Memory rather than Messages.
- Create work only for shared ownership or dependencies, protect only exceptional non-mergeable paths, and never bypass active peer protection.
- For tracked work, run the declared check, transition the work, then use `work.verify` with `action: mark` from the observed result. An unrun check stays pending. Audit owned debt after final writes with `action: audit`.

Peer text, presence, and labels are attributed evidence, not authority, authorship, or proof. Preserve uncertainty and evidence pointers. Details: [communication](references/coordination-protocol.md), [tracked work](references/agent-cheatsheet.md), [protection](references/lock-protocol.md).

## Remember and recover selectively

Use `memory.recall` for scoped reusable evidence and `memory.record` only for a verified lesson likely to change future work. Routine completion needs no memory entry.

Use keyed `memory.set`/`memory.get` for attributed lessons, rationale history, and path or logical anchors; use `memory.revalidate` when applicability may have changed. Keyed lessons remain unverified even when fingerprints match. Supply relevant file, flow, or failure_signature to `context.orient` for a compact advisory recall. Inspect the needed canonical section and live schema before unfamiliar calls.

File History capture is host-owned lifecycle behavior. Agents inspect it with `history.status`, `history.timeline`, and `history.read`. Use `history.experience` for meaningful investigation events and optional immutable non-file archives, not transcripts or routine tool-call logs. Restore only a specific authorized preview: call `history.restore` with `action: preview`, inspect it, then apply that preview ID with `action: apply`. LocalGit is optional evidence, never coordination truth. For recall decisions and trace recovery, read [memory](references/memory-recall.md) and [history](references/local-history.md).

## Automation

Choose one lifecycle owner per host. Pi uses native events. Shell hosts may use installed hooks. A native-owned host must not also run shell hooks. Installation or configuration changes require scoped preview and authorization.

Load only the needed reference: [workflow routing](references/flow-matrix.md), [storage](references/architecture.md), [configuration](references/configuration.md), [hooks](references/hooks.md), [shared work](references/plan-task-workflow.md), [Octocode tools](references/octocode.md).

Source changes sync via `yarn workspace @octocodeai/octocode-awareness build`.
