Load when deciding whether to delegate to subagents, what a worker returns, or who owns a task.

# Delegation

## Delegate only separable work

When a measured gap needs more than one agent, start with one supervisor and bounded specialist workers, not an open network of agents talking to each other.

- Do a trivial subtask or a tightly coupled decision inline. Delegate a subtask only if it can be stated without the parent's conversation and its result checked without it; otherwise do it inline.
- Do not let every agent read the same large context; pass precise task contracts and evidence references.
- Run independent tasks in parallel, dependent ones in sequence.
- Bound recursive delegation by depth and cost, and bound concurrency: take the limits from the task's budget and state them in the brief; with no budget given, ask the owner instead of choosing one.
- Measure whether several agents beat one strong agent on the actual workload: in one study token spend explained 80% of the variance in multi-agent research quality.
- Let several agents contribute findings while one writes: parallel writers make conflicting decisions (Cognition's report).
- Spend first on the specification. Across 1,642 annotated traces (MAST, GPT-4o-era models), failure modes split roughly 44% specification and system design, 32% inter-agent misalignment and 24% missing verification; a better role specification alone raised one system's success 9.4 points. Sources: [MAST](https://arxiv.org/html/2503.13657v3), [Anthropic](https://www.anthropic.com/research/multi-agent-research-system), [Cognition](https://cognition.com/blog/dont-build-multi-agents).

## Return contracts

A subagent returns a bounded result, not its process history.

Return outcome, deciding evidence, changed artifacts, checks and unresolved work as needed by the caller. Match any existing host result schema; do not invent an envelope for a simple handoff. A runtime-validated example is in [typed messages](typed-messages.md#typed-messages).

A successful worker return does not establish correctness: verify the artifact, test or state the next step depends on.

## Task ownership

```text
submitted → working → completed (deliver result)
                    → failed    (return error)
                    → canceled  (stop execution)
```

Simplified: A2A also defines interrupted states such as input-required and auth-required, and a rejected terminal state.

- One accountable owner per task at a time.
- The parent tracks child task IDs and cancellation.
- Terminal tasks do not silently restart.
- After a timeout or a lost reply, follow [delivery](message-delivery.md#reliability-and-delivery).
- A canceled task has not necessarily rolled back its side effects.
- A successful transport response does not prove the business operation succeeded.
