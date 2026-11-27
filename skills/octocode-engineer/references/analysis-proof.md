Load when stating what a number, complexity or finding proves, choosing the proof a finding needs, or ranking findings.

# Analysis: claim limits, proof and ranking

## Claim limits

- Label numbers measured, documented or estimated. Complexity is not latency; locality, I/O, contention, allocation and network work can dominate. Version-specific claims need current documentation or source.
- State approximation error and permitted consistency anomalies beside the choice. Concurrent and memory-safety claims need an execution check, such as a sanitizer, stress test or model checker.
- Match [evidence](evidence.md) to the claim. Distinguish fixture IDs, paths, versions and roots; assert the specific failure. Unresolved scope, identity, flow, runtime impact or measurement keeps a finding a candidate.

## Proof and ranking

- Use the narrowest decisive evidence: source and semantic references for identity, scenario traces for ownership and effects, and execution or measurement for behavior and cost. Graph candidates do not require every tool in sequence.

- Affected scope: graph dependents/path plus semantic references to the changed symbol.
- Layer violation: exact path, importing symbol, authoritative rule, quality impact.
- Dead code / delete: the [removal proof](change-removal.md#delete-junk-and-unify).
- Coupling or god module: graph breadth plus mixed responsibilities and real consumer or change impact, not size.
- Duplication: semantic equivalence, shared invariant, shared change pressure, not text similarity.
- Separation/interface: inward framework/storage/view dependency, invariant owner, consumer contract, policy test seam.
- Hot path: named workload and budget; end-to-end baseline before blaming a symbol; attributed cost (fan-out, I/O, copying, allocation, cache, locks, retries, waits); inclusive/exclusive, p50/tail.
- Efficiency: correctness, latency, throughput, and memory guardrails hold; centrality, call count, or one flame-graph sample is no proof.

- Rank by `evidence strength x consequence x actionability`; report partial scans, inferred roots, warnings, unsupported semantic tools, unmeasured claims.
