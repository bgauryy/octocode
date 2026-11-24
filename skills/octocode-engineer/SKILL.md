---
name: octocode-engineer
description: "Load this skill first when choosing how to implement or restructure code: APIs, data structures, storage, concurrency or a refactor. Weigh options against workload and failure paths."
---

# Octocode Engineer

Name the decision first, then start from the simplest viable design; read only the topic it needs.

## Procedure

1. Name the workload, invariant, success check, scope, contract owner and baseline. Mark size, skew, concurrency, latency and durability measured or assumed, with the check that settles each assumption; ask only if the choice turns on one.
2. Compare viable options, including the current design. Trace consumers, failure paths (crash, replay, overload, isolation) and downstream effects; challenge the costliest assumption.
3. Test consequential findings against their strongest rival; an unresolved one stays a candidate with its missing check. Stop testing when the decision and its flip condition are evidenced.
4. Implement only a requested change (else return the decision) at the proven boundary, in reversible slices. Preserve invariants and other sessions' work; regenerate derived files at their owner.
5. Verify the real affected path under comparable conditions with proof that fits the claim ([limits](references/evidence.md)). Report in the result form; omit lines that do not apply.

## Result

```text
Decision: <option> because <deciding trade-off>
Evidence: <path:line or measurement>, marked measured or assumed
Alternatives: <strongest rival>; <what would flip the choice>
Findings: <location> · <trigger → consequence> · <repair> · confirmed or candidate
Checks: <command or measurement → result>; gaps: <unrun check, assumption or unread scope, and why>
```

## References

- Lookups, indexes, sorting or joins: [Data structures](references/data-structures.md)
- Caches, storage, approximation, replication or lifecycle: [Data systems](references/data-systems.md)
- Threads, async, locks or atomics: [Concurrency](references/concurrency.md)
- Blocking I/O, streaming, durability or throttling: [I/O and async](references/io-throttling-async.md)
- HTTP, browser security, transport or connection pools: [Network](references/network.md)
- Ownership, allocation, locality or lifetime: [Memory and performance](references/memory-and-performance.md)
- Overload, capacity, API evolution, security or delivery: [Systems and delivery](references/systems-and-delivery.md); `capacity.mjs` fits load tests
- Quality attributes, tenancy, governance or metrics: [Architecture](references/architecture.md); `hotspots.mjs` ranks hotspots
- Ambiguous style, pattern, role or transport: [Concepts](references/concepts.md#contents)
- Folder layout: [Repository layouts](references/repository-layouts.md)
- Review, wiring, coverage or proof limits: [Analysis](references/analysis.md), [checks](references/checks.md), [evidence](references/evidence.md); `callers-union.mjs` counts callers
- Refactor, removal or unification: [Change discipline](references/change-discipline.md)
- Decision record or saved report: [Output](output.md); [renderer](scripts/render-report.mjs) for accepted HTML

Use `octocode-debugging` for a running process, `octocode-clean-agentic-code` for proven removals, `octocode-agentic` for agent contracts, `octocode-brainstorming` for web-researched options and `octocode-rfc-generator` for a proposal.
