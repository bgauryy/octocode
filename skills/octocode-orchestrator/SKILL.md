---
name: octocode-orchestrator
description: "Load this skill first when existing agents or workers must be assigned, tracked and recovered on one task. Give each piece an owner, track dependencies, verify results and resume stalled work against one plan."
metadata:
  excluded-hosts: "octocode-code"
---

# Octocode Orchestrator

Recover the user's outcome, acceptance criteria, authority and current work before assigning anything. Reuse the existing plan, workers and task state. Retrieve missing context; ask only for a decision or input that blocks the next dependent action, and continue useful independent work.

Own the user-facing result and integration. Coordinate through the host's available, authorized capabilities; this skill grants no permission to spawn agents, contact peers or mutate external systems. When delegation is unavailable or adds no useful capability, perform the work directly and disclose any missing independent check.

## Coordinator loop

1. **Observe.** Reconcile worker reports with current artifacts. Keep ownership, dependencies, acceptance, evidence and next actions in one tracker. A worker's "done" still needs your check.
2. **Choose.** Identify the unmet condition and the action that can advance it. Map producer and consumer dependencies before releasing work, and check them as [dependency graphs](references/loops.md#dependency-graph) describes: `node <skill-folder>/scripts/dep-graph.mjs --plan <plan.json>`. Use the [execution graph](references/loops.md#execution-graph) for blocked or branching tasks.
3. **Assign or execute.** Give each separable task one owner and a [task contract](references/protocol-catalog.md#delegate-and-coordinate) with scope, authority, inputs, expected result and checks. Ask for each claim, its source and its check status. Add workers for distinct capabilities or independent checks. Serialize conflicting writes and shared builds.
4. **Integrate.** Recheck decisive anchors using the [evidence rules](references/evidence.md). Resolve disagreements with a distinguishing check. Invalidate stale checks and release ready dependencies as results arrive.
5. **Recover or finish.** Use [recovery and resume](references/recovery.md) for stalled tasks, uncertain effects, cancellation or lost context. Continue while an authorized action advances acceptance. Finish when the integrated result passes its required checks.

Use host notifications or supported waits. Request status when a dependency, ownership conflict or missing result needs a decision. Set concurrency from actual capacity and shared resources.

## Connected skills and tools

This skill works alone; load a neighbor only when installed and a need names it.

| Need | Owner |
|---|---|
| Process shape: stages, checkpoints, its ledger and acceptance | `octocode-workflows` |
| One agent's harness, prompts, context, model routing or host binding | `octocode-agentic` |
| Repository claims | `octocode-research` |
| A domain change | Its domain skill; in Octocode, `octocode-dev` owns builds and gates |
| Relay between separate agent systems | `octocode-messages-proxy`, only with messaging authority |
| Bounded text/image work on an installed local model | `octocode-orchestrator-local-worker`; you keep verification and final decisions |

For the design basis and alternatives, read [research notes](references/research.md). To evaluate this skill, use the [behavioral checks](references/checks.md).

Return the outcome first, then decisive evidence, changed artifacts and remaining work. Distinguish verified completion, partial work and a blocker; preserve the host's lifecycle vocabulary. A blocker names the missing prerequisite and owner. On handoff, retain current assignments, handles, unknown effects and the next safe action. Stop owned workers/processes or explicitly hand them over; preserve other sessions' work.
