Load when the [audit checklist](audit-checklist.md) reaches its routing, correctness, security and long-running checks; mark and report each check the same way.

# Audit checklist: routing, correctness and operations

## Model and agent routing

- The common path is a direct call or one loop; fail: an extra agent or model whose measured gap nobody can name ([choose a pattern](architecture.md#choose-a-pattern)).
- Each escalation in a trace follows a trigger of the [escalation policy](model-routing.md#escalation-policy): a failed check or an abstention from the cheaper model, reasoning too complex for it, or an irreversible consequence; fail: a stronger model called with no such trigger.
- Each delegated task is stated without the parent's conversation and its result is checked without it; fail: a subtask that needed the parent's context to run or to verify ([delegation](delegation.md#delegate-only-separable-work)).
- Each worker return carries outcome, deciding evidence, changed artifacts, checks and unresolved work; fail: a transcript returned, or a return accepted with no check of its artifact ([return contracts](delegation.md#return-contracts)).
- The configuration sets a delegation depth, a cost limit and a concurrency limit; fail: a worker that can delegate with no limit ([delegation](delegation.md#delegate-only-separable-work)).

## Correctness and efficiency

- Each claim in a result carries a [grounding label](grounding.md#grounding); fail: an inference or an unrun test reported as confirmed.
- Each completion claim points to a test, run or observation made after the last change; fail: success claimed from plausibility ([stop conditions](execution-control.md#stop-conditions)).
- No two calls in a trace share inputs and results without a state change between them ([loop types](execution-control.md#loop-types)).
- The run ends at a [stop condition](execution-control.md#stop-conditions); fail: calls after acceptance was verified, or a stop with a reachable check left unrun.
- The result gives findings, evidence, residual risk and what remains unverified; fail: process narration in their place, or unchecked scope left unstated ([answer formats](answer-formats.md#response)).

## Security and operations

- Inject an instruction into a fetched page, a tool result, a skill and a peer message; fail: the agent follows it, or its permissions change ([untrusted content](security.md#untrusted-content)).
- Compare each credential and tool scope with what the task uses; fail: a permission no step needed ([risks](security-risks.md#risks-and-mitigations)).
- Send a destructive call without approval; fail: it executes ([sandbox and approval](security.md#sandbox-and-approval)).
- No session combines private data, untrusted content and outbound or state-changing actions without approval; the sandbox covers filesystem and network ([limit what one session combines](security.md#limit-what-one-session-combines)).
- Stop the run mid-task and resume it; fail: a completed side effect repeats, or saved progress is lost ([control loop](control-loop.md#reference-control-loop), [delivery](message-delivery.md#reliability-and-delivery)).
- A dated run records verified success, latency and cost per task against a baseline; fail: a claimed improvement or regression with no comparison ([evaluation](evaluation.md#match-the-claim-to-the-check)).

## Uncertainty and long-running actions

Use the [uncertainty, reversibility and survival checks](mental-models.md#uncertainty-reversibility-and-survival) to inspect invalidating observations, recovery limits and failure containment separately from success. For changing or resumed runs, inspect stale inputs, pending effects and accumulated exposure. Mark missing execution evidence **not assessed**; a plausible scenario is not a proven failure.
