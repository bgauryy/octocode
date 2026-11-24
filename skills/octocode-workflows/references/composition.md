Load when assigning workflow roles or Octocode tools, sequencing work, running parallel write lanes, merging evidence, handing off, or resuming.

# Composition

## Execute an orchestrated workflow

1. Honor the user's scope, actor assignments and sequence. Choose the domain owner and one primary workflow; name an accountable orchestrator and each execution duty. One agent can hold both coordination and execution duties when delegation adds no useful capability or independent check.
2. Reuse or create one ledger and assign bounded Task contracts under [the contract below](#orchestrator-and-actor-contract). Parallelize independent work only when authorized; workers request changed scope or additional roles through the orchestrator. Report unavailable capabilities and affected checks.
3. Recheck deciding anchors, merge observations and accept or reassign rows using [verification](verification.md). A runtime claim needs loaded-artifact identity; uncertain mutations need durable-state reconciliation before retry. Domain browser and sealed-evaluation safeguards remain in force.

## Orchestrator and actor contract

| Duty | Owner and responsibility |
|---|---|
| Coordinate | One orchestrator owns goal, authority, dependencies, assignments, the sole ledger, integration, verification and the user-facing result. |
| Execute | Each actor owns its bounded scope and returns the deciding observations for the assigned work. It creates no competing ledger, accepts no own work as independent proof and does not expand delegation without the orchestrator's authorization. |
| Set domain acceptance | The owner skill retains its methods, permissions and output. |

Roles describe duties, not a minimum agent count. If one agent holds several duties, record any resulting loss of independence instead of inventing a second actor.

| Assignment: Task contract + Definition of Done | Actor return: claim–evidence, keyed to the row | Orchestrator merge |
|---|---|---|
| Objective: goal/scope, target boundary; inputs: relevant Octocode access and anchors; constraints: authority (read/write/process), role; Definition of Done: acceptance condition; next row | Assigned work; deciding source/runtime evidence, relevant conditions, expected and observed result; gap and next action | Re-read deciding anchors, check applicability and identity, retain disproof/uncertainty, update the one ledger |

Use `octocode-agentic` coordination for identity, leases, requests, completion, and durable delivery. A delivered handoff is not a verified result. One source/build owner writes a changed boundary; one process owner changes a target process. Inspectors return evidence rather than racing the writer with fixes.

For an independent check, give the checking actor the requirement, owner contract, and raw inputs before the producer's conclusion when feasible. It derives its own acceptance or distinguishing check and returns primary anchors. Two summaries of the same shared observation do not corroborate each other; the orchestrator records overlapping evidence rather than counting it twice.

Keep the acceptance oracle stable. A builder cannot weaken an assertion or redefine success to pass. An oracle correction needs owner-contract evidence, a versioned comparison, preservation of the old result, and invalidation of affected rows/comparisons; the orchestrator records the change without forcing acceptance.

## Parallel write lanes

When several actors change one repository at once, partition the writing before launch.

| Concern | Rule |
|---|---|
| Ownership | Each Task contract names the paths the actor owns and the owner of each neighboring path. A fix needed outside the actor's paths goes back as a request in its return, never as an edit. Where two lanes must share a file, each makes small exact edits and re-reads the file on conflict. |
| Shared mutable resources | Generated artifacts get one locked regenerate command. The orchestrator owns the build and the lockfile. Lanes share one build cache, never a private target or cache directory each. The [machine budget](machine-budget.md) decides when a heavy job may start. |
| Contract-breaking change | The orchestrator lands the upstream contract change and regenerates before lanes adapt to it. Otherwise one lane owns the change and every consumer it breaks. |
| Half-finished neighbors | A compile or test failure in another lane's files is re-run before it is blamed. If it persists, report it with the file and line. |
| Redirect | When evidence contradicts a lane's direction, send it the measured evidence, the new target, and what to remove. Stopping early costs less than merging the wrong design. |
| User hold | "Plan first" or a design question pauses every edit the plan would change. Lanes continue only the work the plan leaves unchanged. |
| Foreign effects | When something outside the lanes changes shared state (index staging, background jobs), report it. Never stash, reset, or overwrite to hide it. |

Size a lane by its files and checks, not by item count. Once its return is in, re-read the deciding anchors before accepting, as in [Diagnose and replan](verification.md#diagnose-and-replan-from-evidence).

## Octocode tools for every role

The [shared tool-access rule](../SKILL.md#use-octocode-across-workflows) applies to every role. Assign the evidence question and checkout/ref or entry point; let its owner refine the calls. The orchestrator can use the same tools to scope work, resolve disagreements and verify anchors.

Choose tools by the evidence question; `octocode-research`, when installed, adds detailed query selection. The rows below pair source capabilities with each boundary.

| Evidence needed, in any workflow | Useful Octocode capability and application |
|---|---|
| Connect observed behavior to implementation | Local search/read plus AST/LSP: trace a runtime or browser observation through handlers, state, and consumers; inspect scraper extraction and normalization. Pair source evidence with the domain's live observation or replay. |
| Find a resource-flow bottleneck | Local search/read plus AST/LSP: trace instruction loaders, cache keys, invalidation, queues, allocations, and lifetimes. The plumber pairs these anchors with measurements before claiming an efficiency or memory benefit. |
| Check an instruction or evaluation pipeline | Search/read the effective prompt loader, schema, fixture, grader, and result consumer. Inspect only evidence permitted by the evaluation phase; retain held-out boundaries. |
| Resume or independently verify work | Reopen exact source anchors and recheck affected consumers against current identity. Communication tools carry assignments and evidence; Octocode research tools check repository claims in them. |

Apply the [shared evidence rules](evidence.md). Return capability gaps to the orchestrator, or use the domain's supported alternative; an unavailable call is not a completed check.

## Compose and resume

- Reopen the existing ledger and reconcile current source, artifacts, configuration and state. Keep evidence only while its dependency basis still holds; assign the next outstanding row instead of starting a replacement ledger.
- On disagreement, seek a distinguishing observation or record the unresolved tradeoff; agreement and eloquence are not evidence.
- Add a helper only for a distinct check or capability within authority and the user's sequence. Reject a split that merely paraphrases, lacks decisive access or costs more coordination than the work. No nested, unsupervised teams.

When a workflow needs unavailable actor tools, report it as an SBAR: situation (the missing role or tool), background (the workflow and row), assessment (the affected check), recommendation (what to supply or accept). If the orchestrator also executes, state that explicitly and identify any unavailable independent check. A direct task outside this workflow needs no orchestrator wrapper.

Next: choose a concrete split in [role patterns](role-patterns.md) or execute the selected procedure.
