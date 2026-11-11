# Audit checklist

Load when auditing a production agent, harness or agent feature. Mark each check **pass**, **fail** or **not assessed**, with the evidence (trace, config, code location, run). A fail becomes a finding with symptom, mechanism, consequence, fix and the check that proves the fix ([output](../output.md#audit-findings)). Configuration alone supports a candidate; a trace or run supports a proven finding.

## Context and attention

Detail: [context and memory](context-and-memory.md), [retrieval](retrieval-and-grounding.md).

1. Loads only task-relevant context.
2. Uses precise retrieval before broad reads.
3. Reserves context and output headroom.
4. Keeps authoritative instructions and constraints salient, with explicit precedence.
5. Tracks evidence provenance and source revisions.

## Caching and memory

Detail: [caching](context-and-memory.md#caching), [memory](context-and-memory.md#memory), [compaction](context-and-memory.md#compaction).

6. Keeps cacheable prefixes stable.
7. Separates caches from durable memory.
8. Versions and expires cached data.
9. Isolates users and tenants.
10. Preserves decisions and unresolved work through compaction.

## Tools, MCP and skills

Detail: [tools and MCP](tools-and-mcp.md).

11. Tools are descriptive and non-overlapping.
12. Skills are discovered progressively.
13. Schemas and permissions are validated at execution.
14. Uses pagination, filters and range reads.
15. Handles truncation, errors, cancellation and timeouts.

## Model and agent routing

Detail: [models and delegation](models-and-delegation.md).

16. Uses simple execution by default.
17. Escalates model capability only when justified.
18. Delegates bounded, independent tasks.
19. Defines structured subagent return contracts.
20. Bounds recursive delegation and concurrency.

## Correctness and efficiency

Detail: [grounding](retrieval-and-grounding.md#grounding), [execution control](execution-control.md).

21. Distinguishes confirmed, inferred and unknown claims.
22. Verifies actual outcomes with tests or observations.
23. Avoids redundant calls and repeated investigations.
24. Stops when evidence and acceptance criteria suffice.
25. Returns concise findings and remaining risks.

## Security and operations

Detail: [security](security.md), [evaluation](evaluation.md).

26. Treats retrieved data, skills and peer agents as untrusted.
27. Enforces least-privilege execution.
28. Protects destructive actions with approval.
29. Persists safe checkpoints and durable action IDs.
30. Measures task success, latency, cost and regressions.
