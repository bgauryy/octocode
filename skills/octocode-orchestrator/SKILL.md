---
name: octocode-orchestrator
description: "Load first to coordinate several agents through an ongoing task: assigning work, tracking dependencies, merging evidence, recovering stalled loops, resuming interrupted runs."
metadata:
  excluded-hosts: "octocode-code"
---

# Octocode Orchestrator

Recover the user's outcome, acceptance criteria, authority and current work before assigning anything. Reuse the existing plan, workers and task state. Retrieve missing context; ask only for a decision or input that blocks the next dependent action, and continue useful independent work.

Own the user-facing result and integration. Coordinate through the host's available, authorized capabilities; this skill grants no permission to spawn agents, contact peers or mutate external systems. When delegation is unavailable or adds no useful capability, perform the work directly and disclose any missing independent check.

## Coordinator loop

1. **Observe.** Reconcile worker reports, artifacts and current source/runtime state. Track task ownership, dependencies, acceptance, evidence, unresolved questions and the next action in one existing tracker. Keep worker lifecycle separate from evidence: a completed worker can return failed, stale or unverified work. Persist only what a later decision or resume needs.
2. **Choose.** Name the unmet acceptance condition and the observation or action that can advance it. For dependent work, map producer → consumer edges and resolve shared prerequisites first. Use the [execution and dependency graphs](references/loops.md#execution-graph) when decomposing work or diagnosing a blocked loop. Preserve the user's sequence and domain acceptance.
3. **Assign or execute.** Give separable work one accountable owner and a [Task contract with a Definition of Done](references/protocol-catalog.md#delegate-and-coordinate). Pass the requirement, relevant source/artifact references, allowed tools and write scope; include expected checks and return evidence: per item a claim, its anchor, the check run and its status, without restating the assignment. Add workers only for a distinct capability, evidence lane or independent check. Keep coupled changes together and serialize conflicting writes, builds and shared generated outputs. Workers request scope changes through the coordinator.
4. **Integrate.** Inspect returned artifacts and recheck the deciding anchors using the [evidence rules](references/evidence.md). Resolve disagreement with a distinguishing observation. Update the existing tracker, invalidate checks whose inputs changed, and release only dependencies whose required evidence is current. Process useful results as they arrive; unrelated work need not wait for the slowest worker.
5. **Continue, recover or finish.** Continue while a reachable action can advance acceptance within authority and actual host/user limits. More messages, repeated summaries and repeated unchanged checks are not progress. Use [recovery and resume](references/recovery.md) for stalls, uncertain effects, cancellation or context loss. Finish only when the requested result and its required checks hold on the integrated state.

Use event notifications or supported waits for active work. Request status when a dependency, ownership conflict or missing result requires a decision; avoid repeated polling. Bound concurrency by host capacity and shared resources, without inventing fixed round, task or evidence quotas.

## Connected skills and tools

Use these neighbors when installed and relevant; this skill works without installing them. Load the needed branch, not every skill.

| Need | Owner |
|---|---|
| Choose or compose a workflow; reuse its ledger and acceptance | `octocode-workflows` |
| Diagnose the agent harness, prompting, context, model routing or communication setup | `octocode-agentic` |
| Establish repository claims | `octocode-research`; discover available tools through the [MCP/CLI guide](references/octocode-usage.md) |
| Execute or verify a domain change | Its domain skill; for Octocode development, `octocode-dev` owns builds and gates |
| Relay between separate agent systems | `octocode-messages-proxy`, only with the required messaging authority |
| Offload bounded text/image work to an installed local model | `octocode-orchestrator-local-worker`; coordinator retains verification and final decisions |

For the design basis and alternatives, read [research notes](references/research.md). To evaluate this skill, use the [behavioral checks](references/checks.md).

Return the outcome first, then decisive evidence, changed artifacts and remaining work. Distinguish verified completion, partial work and a blocker; preserve the host's lifecycle vocabulary. A blocker names the missing prerequisite and owner. On handoff, retain current assignments, handles, unknown effects and the next safe action. Stop owned workers/processes or explicitly hand them over; preserve other sessions' work.
