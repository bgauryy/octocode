# Audit checklist

Load when auditing a production agent, harness or agent feature. Mark each check **pass**, **fail** or **not assessed**, with the evidence (trace, config, code location, run). Report each fail as an [audit finding](../output.md#audit-findings), which owns its fields and the proven-versus-candidate rule.

## Context and attention

Detail: [context and memory](context-and-memory.md), [retrieval](retrieval-and-grounding.md).

- Loads only task-relevant context.
- Uses precise retrieval before broad reads.
- Reserves context and output headroom.
- Keeps authoritative instructions and constraints salient, with explicit precedence.
- Tracks evidence provenance and source revisions.

## Caching and memory

Detail: [caching](context-and-memory.md#caching), [memory](context-and-memory.md#memory), [compaction](context-and-memory.md#compaction).

- Keeps cacheable prefixes stable.
- Separates caches from durable memory.
- Versions and expires cached data.
- Isolates users and tenants.
- Preserves decisions and unresolved work through compaction.

## Tools, MCP and skills

Detail: [tools and MCP](tools-and-mcp.md).

- Tools are descriptive and non-overlapping.
- Skills are discovered progressively.
- Schemas and permissions are validated at execution.
- Uses pagination, filters and range reads.
- Handles truncation, errors, cancellation and timeouts.

## Model and agent routing

Detail: [models and delegation](models-and-delegation.md).

- Uses simple execution by default.
- Escalates model capability only when justified.
- Delegates bounded, independent tasks.
- Defines structured subagent return contracts.
- Bounds recursive delegation and concurrency.

## Correctness and efficiency

Detail: [grounding](retrieval-and-grounding.md#grounding), [execution control](execution-control.md).

- Distinguishes confirmed, inferred and unknown claims.
- Verifies actual outcomes with tests or observations.
- Avoids redundant calls and repeated investigations.
- Stops when evidence and acceptance criteria suffice.
- Returns concise findings and remaining risks.

## Security and operations

Detail: [security](security.md), [evaluation](evaluation.md).

- Treats retrieved data, skills and peer agents as untrusted.
- Enforces least-privilege execution.
- Protects destructive actions with approval.
- Persists safe checkpoints and durable action IDs.
- Measures task success, latency, cost and regressions.

## Uncertainty and long-running actions

Use the [uncertainty, reversibility and survival checks](mental-models.md#uncertainty-reversibility-and-survival) to inspect invalidating observations, recovery limits and failure containment separately from success. For changing or resumed runs, inspect stale inputs, pending effects and accumulated exposure. Mark missing execution evidence **not assessed**; a plausible scenario is not a proven failure.
