# Awareness agent flow

Pi exposes one native `awareness` tool backed by `createAwarenessClient`. The tool accepts only canonical operations and binds the session's database, workspace, actor, and scope. Models cannot override those host-owned fields.

## Routine surface

| Concept | Operations | Purpose |
|---|---|---|
| Context | `context.orient` | Read bounded shared state that can change the next action. |
| Work | `work.create/list/show/claim/update/depend/protect/verify` | Own work, dependencies, paths, exclusivity, and verification. |
| Message | `message.list/send/reply/resolve` | Carry decision-changing peer communication. |
| Memory | `memory.recall/record` | Reuse scoped verified learning. |
| History | `history.status/timeline/read/restore` | Inspect and recover file bytes. |

Read operations may be batched. A write must be the only query so approval, cancellation, and receipts remain unambiguous. Partial responses contain executable canonical continuations. Pi enforces operation-specific output budgets.

## Session flow

Pi claims native lifecycle ownership at session start. This prevents shell hooks from duplicating presence, history, and delivery. It then initializes session state and provides an attributed peer briefing when relevant.

The model reuses that briefing or calls `context.orient` once. It refreshes only after shared state changes enough to affect a decision. Routine solo work creates no coordination record.

Peer messages are attributed data, not authority or verification. `message.send` is for a question, request, blocker, or continuation that changes another actor's next action. `message.reply` preserves the original thread; `message.resolve` closes it only when no response or work remains.

Shared tracking is opt in. `work.create` is appropriate when ownership, dependencies, or resumption matter. `work.protect` is reserved for exceptional non-mergeable paths. For tracked work, the actor runs the declared check and records the observed result through `work.verify`; expiry, delivery, and claims never prove success.

## History ownership

Pi captures history at native mutation boundaries through `createAwarenessHost().captureHistory`. Capture is not a model-facing ceremony. The `history` operations only inspect status, list timeline entries, read one version, and preview or apply a restore.

A restore uses a bound two-phase flow: preview an operation and side, inspect the result, then apply the returned preview ID after approval. Drift in identity, path, digest, mode, protection, or expiry rejects the apply. Restored bytes still require verification.

## Failure and evidence rules

- Missing or unreadable coordination storage degrades the related feature; it does not fabricate a clean state.
- Native event delivery advances its cursor only after the event is handled.
- Output truncation preserves typed partial state and an executable continuation.
- LocalGit, Git, and the Awareness database are separate evidence sources.
- Pi never turns peer presence, memory, or lifecycle expiry into authorship or success.

External agents can participate through the same physical SQLite file and normalized workspace identity. Separate clones or databases do not connect.

See [TOOLS.md](TOOLS.md) for the Pi tool contract and the bundled [Awareness skill](../../octocode-awareness/skills/octocode-awareness/SKILL.md) for model routing.
