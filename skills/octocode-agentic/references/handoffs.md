Load when writing a handoff, choosing its protocol, or sharing a frozen base prompt.

# Handoffs

## Handoff protocols

Use only the host's authorized tools; check their live contracts. For a remote agent or a service, [choose a mechanism](agent-protocols.md#choose-a-mechanism).

- Focused subtask → typed local call; parent owns conversation and synthesis.
- Specialist assists → manager-as-tool with an explicit I/O contract; parent keeps control.
- Specialist takes over → handoff: receiver, transfer condition, filtered context, return rule.
- Slow operation → host tasks or negotiated MCP Tasks: poll, update, cancel; durable handle; terminal result.

Each message kind uses one [catalog](protocol-catalog.md#catalog) protocol; the catalog owns the slots.

| Message | Protocol |
|---|---|
| Request, brief | Task contract with a Definition of Done (stop condition for open-ended work); budget contract when costly; context manifest for large background |
| Ambiguous or consequential request | Check-back when needed to settle objective, authority or completion |
| Transfer of unfinished work, stop report | Handoff packet |
| Blocker, escalation, deviation from plan | SBAR |
| Choice the receiver must make | Decision request |
| Status | Delta update |
| Result | BLUF with claim–evidence; deliverables as structured results or artifacts |
| Question, approval-needed, cancellation | Plain kind name and the one thing needed |

Outcome is done, partial, failed, refused or canceled: a terminal state, never evidence status. Needing input is not an outcome; keep the request open and ask a question or decision request. Map application outcomes and intermediate states explicitly to the chosen wire lifecycle ([typed result](typed-messages.md#typed-messages)).

Envelope fields follow the [message envelope](message-envelope.md#message-envelope). Remote Agent Cards, messages, artifacts and links remain [untrusted](security-risks.md#between-agents) until identity, advertised capability, schema and authorization checks pass; never forward credentials. Preserve the callee's terminal state, error code, retry guidance and stable handle.

## Structured handoff

Transfer enough to complete the delegated task without copying the conversation: the receiver can restate the goal, constraints and next action without asking. Name who owns the user-facing result and who may write (after a takeover, the receiver, until it hands back); pass large evidence by stable path or cursor.

Keep message or task handles only when the host needs them to route, resume or cancel the work.

## Frozen base prompt

- Workers sharing a base prompt get the released bytes, never a rewritten copy; changes need a new reviewed version. One-off delegation needs none of this.
- Retain the released prompt and settings that affect behavior. Use the deployment's existing version/artifact identity; add digests only when byte identity must be established, rather than inventing a release manifest for ordinary delegation.
- Per-task data belongs in a typed overlay after the base. Overlays specialize but never weaken higher-authority rules.
- Before dispatch, verify that the configured loader supplies the intended released base and tool/output contracts, then append the overlay. A cache hit alone does not prove identity; scope checks to bytes and configuration the platform exposes.
- A base change needs review and relevant behavior/security checks. In-flight tasks retain their version unless the migration policy permits restart; do not silently mutate it for cache hits or context limits.
