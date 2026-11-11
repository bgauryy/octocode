---
name: octocode-engineer
description: "Engineering and architecture cheat sheet and review method: choose or review data structures, storage, caches, concurrency, memory, performance, reliability, APIs, security, testing, module boundaries, coupling, layering and refactors. Gives trade-offs, failure modes, proof and the check that settles a choice."
---

# Octocode Engineer

A thinking checklist, a lookup and a review method for any engineering choice, from a hash map to a service boundary. Name the workload, the invariant and the measure of success first, pick the simplest option that meets them, and prove it with a check.

| Task | Start with |
|---|---|
| Choose or compare a technical option | [Think it through](#think-it-through), then the matching page's **Choose quickly** table |
| Review architecture, boundaries, coupling or a refactor in code | [Steps](#steps), [claims](#when-making-a-claim) and [gates](#gates) |
| Name what code is: a style, pattern, role or transport | [Concepts](references/concepts.md), including its look-alike terms |
| Choose or govern an architecture | [Architecture](references/architecture.md): quality attributes, tenancy, fitness functions, metrics, checklist |

## Think it through

| Decide | Record |
|---|---|
| Goal | Required outcome, beneficiary and excluded scope |
| Workload | Point/range reads, scans, writes, deletes; size/growth, skew/hot keys, concurrency, p50/p99 latency, durability, retention |
| Invariants | Uniqueness, ordering, isolation, exactly-once effects, memory safety, trust/tenant boundaries, dependency direction |
| Options | Two or three candidates, including the simplest: hash map, mutex, indexed table, process or modular monolith |
| Trade-offs | Correctness first; latency, throughput, memory, operational cost, reversibility |
| Failure modes | Mid-write crash, retry/duplicate, partition, overload, slow dependency, clock skew, malformed/malicious input, wrong-way dependency |
| Proof | Production-like benchmark, query plan, sanitizer, property/load test, restore drill or build-enforced boundary rule |

Complexity is a model, not latency: cache locality, I/O, contention, allocation and network round trips often dominate. Measure before optimizing and prefer the design whose failure you can detect.

## Steps

For a review or refactor of real code:

1. Frame the decision, use case, benefit, scope and contract owner. Pin the revision and establish a baseline before claiming a gain.
2. Trace the [critical paths](references/analysis.md#explore-the-graph-and-find-critical-paths) to their consumers. Record shapes, owners, invariants and failure modes. Apply the [review angles](references/checks.md#review-angles) to the affected code. Match coverage to the workflow.
3. Give each finding a hypothesis test: the claim, its strongest rival, the distinguishing check and one result, **confirmed**, **dismissed** or **candidate**. Name the missing check for a candidate. Keep healthy invariants and disproof.
4. For a review or an unproven seam, return findings and a plan. Before any authorized edit, apply the [gates](#gates) and [change discipline](references/change-discipline.md). Make the smallest change at the proven boundary with a focused test that detects the failure.
5. Verify affected wiring through the production path. Inspect exit status and payload errors. Compare before and after on the same measure. If a check fails or evidence goes stale, diagnose and retry while a useful authorized check remains; report the gap when blocked or out of budget.

## When making a claim

- Label each number measured, documented or estimated, and asymptotic costs as models. A claim about a specific database, library, runtime or version needs its current documentation or source, not this sheet.
- Approximate structures need an accepted error rate stated up front; consistency patterns need the anomaly they allow stated next to them. Concurrency and memory claims need an execution check (sanitizer, race detector, stress test or model checker), not inspection alone.
- Compare [wiring views](references/analysis.md#model-the-wiring) and label rules declared, observed or inferred. Use [concepts](references/concepts.md) to tell a style, pattern, role or transport from its look-alikes. Report the observed mechanism and its consequence; a pattern present, absent or misnamed is no finding by itself.
- For topology, when discovery lists `graphBuild`, run it on the directory, then `graphQuery` with the same `path` (`octocode schema graphQuery --view variants`); otherwise use `lspSearch` and `astSearch`. Edges suggest relationships. A type-only cycle needs separate runtime evidence.
- For caller counts or absence, union graph results with `lspSearch` callers and check [coverage](references/analysis.md#choose-the-narrowest-evidence-lane) before claiming completeness: either tool can miss callers.
- Match [evidence](references/evidence.md) to the claim. Distinguish IDs, paths, versions and roots in fixtures and assert the specific failure. Keep scope, flow, identity, runtime impact or measurement marked **candidate** while unresolved.

## Gates

- For public-contract rewires, cross-package moves, schema or storage migrations, deletes or renames, proceed only with exact authorization. Otherwise make a decision request with options, trade-offs, a recommendation and the default if unanswered.
- Clean what the change caused or touches. Broader refactoring needs evidence, an acceptance check and authorization.
- Update only repo-required bookkeeping and regenerate derived files. Make unimplemented reachable paths fail explicitly and test observable outcomes. Coordinate overlapping edits. Never stash, reset, overwrite or discard others' work.

## Resources

Each topic page starts with a **Choose quickly** table: problem, starting point, what to watch. Read the detail tables only for the candidates you are weighing.

| When | Read | Why |
|---|---|---|
| Choosing a lookup structure, index, sort, join or range-query structure; estimating complexity | [Data structures and algorithms](references/data-structures.md) | Costs, caveats and a verified reverse-lookup pattern |
| Caching, storage engines, approximate counting, replication, consistency, integrity, data pipelines and governance | [Data systems](references/data-systems.md) | Eviction, LSM and MVCC internals, sketches with error bounds, consensus, CDC and event time |
| Threads, async, locks, atomics, memory ordering, concurrency bugs | [Concurrency](references/concurrency.md) | Primitives, failure catalog, model choice, a sanitizer-checked ordering example |
| Allocation, ownership, references, memory safety, CPU caches, latency numbers, profilers | [Memory and performance](references/memory-and-performance.md) | Allocators, ownership by language, hardware costs and the tools that find each bug class |
| Overload, resilience, capacity math, API evolution, security and identity, SRE, deployment, code design, debugging method | [Systems and delivery](references/systems-and-delivery.md) | Patterns with their watch points, formulas with worked numbers |
| Choosing or governing an architecture: quality attributes, multi-tenancy, fitness functions, metrics, review checklist, principles | [Architecture](references/architecture.md) | Measurable requirements and executable structural rules |
| Naming a style, pattern, role, transport or mechanism; testing, debugging and bug vocabulary; repository layouts | [Concepts](references/concepts.md) | Look-alike terms, a decision guide and the evidence that separates them |
| Tracing critical paths, modelling wiring, choosing an evidence lane, ranking proof | [Analysis](references/analysis.md) | Graph, LSP and AST lanes with their coverage limits |
| Reviewing protocols, contracts, data flow, algorithms, efficiency, resources or modularity in code | [Checks](references/checks.md) | Review angles with the defect each one finds |
| Refactoring, deleting or unifying code | [Change discipline](references/change-discipline.md) | Seams, slices, verification and removal bars |
| Matching proof to a claim; using Octocode tools and live schemas | [Evidence](references/evidence.md), [MCP/CLI guide](references/octocode-usage.md) | What each kind of evidence can and cannot prove |
| Writing the answer, review or saved report | [Output](output.md) | BLUF, design answer, decision record, report contract |

## Related skills

- `octocode-research` for repository, GitHub, package or symbol facts and live schemas.
- `octocode-workflows` to choose or coordinate a workflow: [roles and handoffs](../octocode-workflows/references/composition.md#orchestrator-and-actor-contract), [verification](../octocode-workflows/references/verification.md) for artifact identity, one coverage ledger and recovery, and [whole-system traversal](../octocode-workflows/references/single-actor-traversal.md), [runtime surgery](../octocode-workflows/references/runtime-surgery.md) or [AST/LSP techniques](../octocode-workflows/references/ast-lsp-workflows.md) for that procedure. `octocode-agentic` for agent transport identities, leases or messages.
- `octocode-graph-view` for a saved graph brief or interactive view; `octocode-rfc-generator` for a reviewed decision document; `octocode-clean-agentic-code` for behavior-preserving removal; `octocode-brainstorming` to research open options; `octocode-agentic` to design a fair benchmark; `octocode-pr-review` for a merge verdict.

## Output

Follow [output.md](output.md), including its [decision record](output.md#decision-record) for consequential work. Use [render-report.mjs](scripts/render-report.mjs) for accepted HTML rendering or manifest discovery.
