# Review method: steps, claims and gates

Load when reviewing or refactoring real code, stating a technical claim, or about to edit. Findings carry a hypothesis-test result; edits pass the gates first.

## Steps

For a review or refactor of real code:

1. Frame the decision, use case, benefit, scope and contract owner. Pin the revision and establish a baseline before claiming a gain.
2. Trace the [critical paths](analysis.md#explore-the-graph-and-find-critical-paths) to their consumers. Record shapes, owners, invariants and failure modes. Apply the [review angles](checks.md#review-angles) to the affected code. Match coverage to the workflow.
3. Give each finding a hypothesis test: the claim, its strongest rival, the distinguishing check and one result, **confirmed**, **dismissed** or **candidate**. Name the missing check for a candidate. Keep healthy invariants and disproof.
4. For a review or an unproven seam, return findings and a plan. Before any authorized edit, apply the [gates](#gates) and [change discipline](change-discipline.md). Make the smallest change at the proven boundary with a focused test that detects the failure.
5. Verify affected wiring through the production path. Inspect exit status and payload errors. Compare before and after on the same measure. If a check fails or evidence goes stale, diagnose and retry while a useful authorized check remains; report the gap when blocked or out of budget.

## When making a claim

- Label each number measured, documented or estimated. Complexity is a model, not latency: cache locality, I/O, contention, allocation and network round trips often dominate. A claim about a specific database, library, runtime or version needs its current documentation or source, not this sheet.
- Approximate structures need an accepted error rate stated up front; consistency patterns need the anomaly they allow stated next to them. Concurrency and memory claims need an execution check (sanitizer, race detector, stress test or model checker), not inspection alone.
- Compare [wiring views](analysis.md#model-the-wiring) and label rules declared, observed or inferred. Use [concepts](concepts.md) to tell a style, pattern, role or transport from its look-alikes. Report the observed mechanism and its consequence; a pattern present, absent or misnamed is no finding by itself.
- Topology, callers, counts and absence: pick the [evidence lane](analysis.md#choose-the-narrowest-evidence-lane). Graph edges and AST matches are candidates; `lspSearch` resolves identity; neither proves completeness without its coverage. A type-only cycle needs separate runtime evidence.
- Match [evidence](evidence.md) to the claim. Distinguish IDs, paths, versions and roots in fixtures and assert the specific failure. Keep scope, flow, identity, runtime impact or measurement marked **candidate** while unresolved.

## Gates

- For public-contract rewires, cross-package moves, schema or storage migrations, deletes or renames, proceed only with exact authorization. Otherwise make a decision request with options, trade-offs, a recommendation and the default if unanswered.
- Clean what the change caused or touches. Broader refactoring needs evidence, an acceptance check and authorization.
- Update only repo-required bookkeeping and regenerate derived files. Make unimplemented reachable paths fail explicitly and test observable outcomes. Coordinate overlapping edits. Never stash, reset, overwrite or discard others' work.

