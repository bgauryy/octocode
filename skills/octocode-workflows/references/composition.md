Load when assigning workflow roles, sequencing work, merging evidence, handing off, or resuming. A concrete split of duties: [role patterns](role-patterns.md). Several actors writing one repository: [parallel write lanes](parallel-write-lanes.md). Octocode tools per role: [role tools](role-tools.md).

# Composition

## Execute an orchestrated workflow

1. Honor the user's scope, actor assignments and sequence. Choose the domain owner and one primary workflow; name an accountable orchestrator and each execution duty. One agent can hold both coordination and execution duties when delegation adds no useful capability or independent check.
2. Reuse or create one ledger and assign bounded Task contracts under [the contract below](#orchestrator-and-actor-contract). Parallelize independent work only when authorized; workers request changed scope or additional roles through the orchestrator.
3. Recheck deciding anchors, merge observations and accept or reassign rows using [verification](verification.md): a runtime row needs its [identity guard](verification.md#automatically-check-artifact-identity), and an uncertain mutation its [durable-state reconciliation](verification.md#diagnose-and-replan-from-evidence) before any retry.

## Orchestrator and actor contract

| Duty | Owner and responsibility |
|---|---|
| Coordinate | One orchestrator owns goal, authority, dependencies, assignments, the sole ledger, integration, verification and the user-facing result. |
| Execute | Each actor owns its bounded scope and returns the deciding observations for the assigned work. It creates no competing ledger, accepts no own work as independent proof and does not expand delegation without the orchestrator's authorization. |
| Set domain acceptance | The owner skill retains its methods, permissions and output. |

Roles describe duties, not a minimum agent count. If one agent holds several duties, including an orchestrator that also executes, state it and record the lost independent check in the row's Gap or next action cell instead of inventing a second actor.

| Assignment: Task contract + Definition of Done | Actor return: claim–evidence, keyed to the row | Orchestrator merge |
|---|---|---|
| Objective: goal/scope, target boundary; inputs: relevant Octocode access and anchors; constraints: authority (read/write/process), role; Definition of Done: acceptance condition; next row | Assigned work; deciding source/runtime evidence, relevant conditions, expected and observed result; gap and next action | Re-read deciding anchors, check applicability and identity, retain disproof/uncertainty, update the one ledger |

Use `octocode-agentic` coordination for identity, leases, requests, completion, and durable delivery. A delivered handoff is not a verified result. One source/build owner writes a changed boundary; one process owner changes a target process. Inspectors return evidence rather than racing the writer with fixes.

For an independent check, give the checking actor the requirement, owner contract, and raw inputs before the producer's conclusion when feasible. It derives its own acceptance or distinguishing check and returns primary anchors. Two summaries of the same shared observation do not corroborate each other; the orchestrator records overlapping evidence rather than counting it twice.

Keep the acceptance oracle stable. A builder cannot weaken an assertion or redefine success to pass. An oracle correction needs owner-contract evidence, a versioned comparison, preservation of the old result, and invalidation of affected rows/comparisons; the orchestrator records the change without forcing acceptance.

## Compose and resume

- Reopen the existing ledger and reconcile current source, artifacts, configuration and state. Keep evidence only while its dependency basis still holds; assign the next outstanding row instead of starting a replacement ledger.
- On disagreement, seek a distinguishing observation or record the unresolved tradeoff; agreement and eloquence are not evidence.
- Add an actor only for a distinct check, capability or evidence lane within authority and the user's sequence. Reject a split that merely paraphrases, lacks decisive access or costs more coordination than the work. No nested, unsupervised teams.

When a workflow needs unavailable actor tools, report it as an SBAR: situation (the missing role or tool), background (the workflow and row), assessment (the affected check), recommendation (what to supply or accept). A direct task outside this workflow needs no orchestrator wrapper.
