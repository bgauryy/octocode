---
name: octocode-orchestrator
description: "Load this skill first when coordinating agents on an ongoing task. Assign owners, track dependencies, verify their results and resume stalled work against one plan."
metadata:
  excluded-hosts: "octocode-code"
---

# Octocode Orchestrator

Recover the user's outcome, acceptance criteria, authority and current work before assigning anything. Reuse the existing plan, workers and task state. Retrieve missing context; ask only for a decision or input that blocks the next dependent action, and continue useful independent work.

Own the user-facing result and integration. Coordinate through the host's available, authorized capabilities; this skill grants no permission to spawn agents, contact peers or mutate external systems. When delegation is unavailable or adds no useful capability, perform the work directly and disclose any missing independent check.

## Coordinator loop

1. **Observe.** Reconcile worker reports with current artifacts. Keep ownership, dependencies, acceptance, evidence and next actions in one tracker. Verify the work even if its worker has finished.
2. **Choose.** Identify the unmet condition and the action that can advance it. Map producer and consumer dependencies before releasing work. Use [execution and dependency graphs](references/loops.md#execution-graph) for blocked or branching tasks.
3. **Assign or execute.** Give each separable task one owner and a [task contract](references/protocol-catalog.md#delegate-and-coordinate) with scope, authority, inputs, expected result and checks. Ask for each claim, its source and its check status. Add workers for distinct capabilities or independent checks. Serialize conflicting writes and shared builds.
4. **Integrate.** Recheck decisive anchors using the [evidence rules](references/evidence.md). Resolve disagreements with a distinguishing check. Invalidate stale checks and release ready dependencies as results arrive.
5. **Recover or finish.** Use [recovery and resume](references/recovery.md) for stalled tasks, uncertain effects, cancellation or lost context. Continue while an authorized action advances acceptance. Finish when the integrated result passes its required checks or report the missing prerequisite.

Use host notifications or supported waits. Request status when a dependency, ownership conflict or missing result needs a decision. Set concurrency from actual capacity and shared resources.

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
