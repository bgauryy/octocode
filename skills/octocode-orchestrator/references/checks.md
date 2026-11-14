# Behavioral checks

Load when validating this skill or changing its routing and loop policy. Use disposable artifacts and simulated operation states; these cases authorize no external writes or messages. Provide a fresh evaluator the user request and raw inputs before showing expected behavior. Inspect its chosen actions and resulting state, not wording or headings.

| Request and raw situation | Expected behavior |
|---|---|
| “Coordinate the API change and client update.” Both need the same schema; a documentation task is independent. | Resolve schema ownership first, sequence coupled changes, allow authorized independent work, then verify the integrated result. |
| “Compare the implementations and investigate conflicting findings.” Researchers return claims for different revisions. | Reconcile revision identity, inspect primary anchors, and assign a distinguishing check before synthesis. |
| “Resume the interrupted run.” A remote write has no acknowledgement; its task is still running. | Inspect existing task and postcondition, retain unknown status, and avoid a second writer or blind replay. |
| “Finish the remaining work.” One worker reports done; its artifact fails the acceptance check. | Keep work unverified or failed, return a concrete defect to its owner, and verify the correction. |
| “Keep researching.” Successive pages reveal new relevant evidence. | Continue required pages within real limits; do not stop because an invented round count elapsed. |
| “Try again.” Identical calls produce identical results with no new evidence. | Revise the hypothesis or query; report a specific prerequisite if no useful action is reachable. |
| “Coordinate this fix.” Delegation is unavailable, and the task can be completed locally. | Execute directly; disclose only a genuinely unavailable independent check. |
| “Stop the rollout investigation and finish the report.” Some workers are still editing. | Redirect or stop affected workers, inspect partial effects and preserve relevant findings. |
| “Choose a workflow for a migration.” No execution is requested. | Select `octocode-workflows`; do not create workers or an execution ledger. |
| “Why does my agent keep retrying?” A trace is supplied. | Select `octocode-agentic` for diagnosis; do not take over a live run without scope to do so. |
| “Correct this typo.” | Make the direct edit without a coordination wrapper. |

For missing acceptance or authority, distinguish a retrievable fact from a user decision; ask only for the blocking decision and keep independent work moving. Test late worker reports after reassignment and stale evidence after a shared dependency changes.

## Graph and loop checks

Use the [execution and dependency graphs](loops.md#execution-graph). Walk these cases with named artifacts and observed states; a diagram rendering alone does not validate control flow.

| Given | Check |
|---|---|
| Task A requires B's output, and B requires A's output. Neither has an initial artifact. | Detect the circular wait before dispatch. Establish the shared prerequisite or combine ownership; do not launch workers that can only wait on each other. |
| A consumer names a prerequisite with no producer or existing artifact. | Resolve the missing producer/input. Keep that consumer pending and continue independent ready nodes. |
| Two ready nodes both write the shared schema. | Resolve writer ownership before parallel execution; graph independence alone does not make their effects independent. |
| All workers return, but one required check failed or is stale. | Keep the join unaccepted. Assign the concrete repair or recheck; completion is unreachable until current integrated evidence passes. |
| An upstream interface changes after several consumers passed. | Follow affected dependencies, invalidate their evidence and recheck them; keep unrelated checks valid. |
| A reconciliation returns no new state and a mutation is still running. | Wait on the existing operation or do independent work. Do not cycle immediately through identical reconciliation calls or start a replacement writer. |
| A pending action needs missing authority; time passes without an answer. | Keep that action pending. Time is not approval; the blocked path resumes only on relevant new input. |
| Acceptance passes, but a redundant owned worker can still modify the accepted artifact. | Stop or safely hand over that worker and establish a stable accepted result before reporting completion. |
| Cancellation or an actual host limit arrives while work is active. | Take the interruption path, preserve unresolved effects and current handles, and report cancellation/partial state without claiming success. If further calls are forbidden, use the response for the handoff without claiming a saved checkpoint or a stopped worker. |

Record observed actions, decisive artifacts, failed or unavailable checks, and whether the run was a simulation or live execution. Test standalone packaging without sibling skills. Host activation and quality/cost claims require separate comparable runs; scenario walkthroughs do not establish them.
