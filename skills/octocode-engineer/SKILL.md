---
name: octocode-engineer
description: "Load this skill first when choosing, designing or reviewing how software should be built: architecture, data structures, storage, caching, concurrency, sync vs async, I/O, streaming, throttling, networking, memory, APIs, reliability or refactors. Load it before proposing a design. Use octocode-debugging for observed runtime failures."
---

# Octocode Engineer

Name the workload, invariant and observable success check. Start with the simplest viable design; read only the topic needed for the decision.

## Procedure

1. Establish scope, contract owner and baseline. Separate measured facts from assumptions about size, skew, concurrency, latency and durability.
2. Compare viable options, including keeping the current design. Trace consumers, failure paths and downstream effects; challenge the assumption whose failure would be most costly.
3. Challenge consequential findings against their strongest rival. Separate confirmed defects from unresolved candidates and name the missing check. Graph/AST matches need source and semantic confirmation; absence needs coverage.
4. Implement an authorized change at the proven boundary, in reversible slices. Preserve invariants and other sessions' work; regenerate derived files at their owner.
5. Verify the real affected path under comparable conditions. Report the decision, evidence, alternatives, checks and remaining gaps. Fewer lines or calls alone do not prove an improvement.

For consequential choices, include crash, replay, overload or isolation failures that could invalidate the design. Match the proof to the claim: complexity is not latency, and source inspection alone cannot establish concurrent or memory safety.

## Conditional references

| Decision | Read |
|---|---|
| Lookup, index, sorting, joins or complexity | [Data structures](references/data-structures.md) |
| Cache, storage, approximation, replication or data lifecycle | [Data systems](references/data-systems.md) |
| Threads, async, locks or atomics | [Concurrency](references/concurrency.md) |
| Sync vs async, blocking I/O, durability, streaming vs full results, fan-out limits, throttling or debounce | [I/O, streaming, async and throttling](references/io-throttling-async.md) |
| HTTP, WebSocket, SSE, cookies, CORS, browser security headers, TLS, DNS or connection pools | [Network](references/network.md) |
| Allocation, ownership, locality, memory safety or leaks in a long-lived process (profiling captures: `octocode-debugging`) | [Memory and performance](references/memory-and-performance.md), [leak hunting](references/memory-and-performance.md#leaks-in-long-lived-managed-runtimes) |
| Overload, capacity, API evolution, security or delivery | [Systems and delivery](references/systems-and-delivery.md) |
| Quality attributes, tenancy, governance or metrics | [Architecture](references/architecture.md) |
| Ambiguous style, pattern, role or transport | [Concepts](references/concepts.md#contents) |
| Folder organization | [Repository layouts](references/repository-layouts.md) |
| Code review, wiring or evidence coverage | [Analysis](references/analysis.md), [checks](references/checks.md) |
| Refactor, removal or unification | [Change discipline](references/change-discipline.md) |
| Proof limits or Octocode calls | [Evidence](references/evidence.md), [MCP/CLI guide](references/octocode-usage.md) |
| Decision record or saved report | [Output](output.md); run the [renderer](scripts/render-report.mjs) for accepted HTML |

Use `octocode-research` for source facts, `octocode-clean-agentic-code` for behavior-preserving cleanup, `octocode-workflows` for actor coordination and its [verification](../octocode-workflows/references/verification.md), [whole-system traversal](../octocode-workflows/references/single-actor-traversal.md), [runtime surgery](../octocode-workflows/references/runtime-surgery.md) and [AST/LSP techniques](../octocode-workflows/references/ast-lsp-workflows.md), `octocode-agentic` for agent contracts and `octocode-rfc-generator` for a full proposal. These skills are optional; the local references cover this skill's review and verification route.
