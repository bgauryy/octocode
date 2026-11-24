# Recovery and resume

Load for a stalled loop, missing worker result, conflicting writes, interruption, cancellation or uncertain side effect. Reconcile facts before dispatching replacement work.

| Observation | Next action |
|---|---|
| Same action, unchanged inputs and unchanged result | Identify the missing information or failed hypothesis; change the question, approach or owner. Stop repeating that attempt while continuing useful work. |
| Valid transient failure | Use the tool/provider's retry contract and existing limits. Distinguish a safe read retry from a write whose outcome is unknown. |
| Worker is silent or a wait timed out | Inspect its existing task handle and artifacts through supported status tools. A timeout is not proof the worker stopped. |
| Two writers need the same boundary | Pause conflicting writes, inspect current changes and assign one owner or sequence. Never overwrite or stash another session's work. |
| Worker result no longer matches current inputs | Mark the affected evidence stale and recheck its dependents; retain unrelated evidence with an unchanged basis. |
| Workers disagree | Check source identity and assumptions, then run the observation that distinguishes their claims. Do not choose by majority or confidence alone. |
| Dependency, authority or input is missing | Report an SBAR through the [protocol catalog](protocol-catalog.md#report-and-hand-off), request the specific prerequisite, and continue independent work. |

## Unknown effects

A lost acknowledgement can follow a successful write. Query durable task/operation state and inspect the postcondition before retrying, compensating or starting a replacement writer. Preserve the original operation identity; rely on deduplication only when the executing service supports it. A local checkpoint does not prove an external action happened exactly once.

If the effect remains unknown, keep dependent work unverified. Request the missing access or decision when needed; do not fabricate a success or failure. Cancellation does not imply rollback: inspect partial artifacts and effects, and retain an explicit owner for unresolved work.

## Resume from current state

Recover the existing goal and acceptance, constraints and approvals, assignments and dependencies, task/process handles, evidence references, unresolved effects and next useful action. Reopen the actual artifacts before trusting an old completion flag. Preserve valid completed work; invalidate only evidence whose relevant source, configuration, fixture or runtime changed.

Reuse active workers where possible. Before replacing an uncertain writer, establish that it stopped or that the host prevents it from writing; otherwise continue only independent work. Never silently restart terminal tasks. Record reassignment in the existing tracker so late results can be checked against the current assignment instead of overwriting newer work.

On a user redirect, update acceptance and affected assignments together, stop obsolete work through supported handles, and preserve work still in scope. On cancellation, stop owned work, reconcile effects and report the remaining state. Before compaction or handoff, checkpoint what the next coordinator needs in the host's existing task store; when a file is needed and no location exists, write `checkpoint.json` (goal, acceptance, plan, handles, evidence, unresolved effects, next actions; shape in [dep-graph](../scripts/dep-graph.mjs) `--help`) under `<workspace>/.octocode/octocode-orchestrator/tmp/<run>/`. Run `node <skill-folder>/scripts/dep-graph.mjs --validate-checkpoint <checkpoint.json>` before handing off and fix each entry in `errors`, such as a running task without a handle or an accepted task downstream of an unknown effect; on resume, run it again and dispatch from its `ready` list. Keep credentials and unrelated transcripts out of it.
