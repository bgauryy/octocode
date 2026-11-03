---
name: octocode-architect
description: "Review architecture, module dependencies, cycles, layering, coupling and boundaries; plan or implement a justified split or refactor, identify patterns and roles, and prove their consequences."
---

# Octocode Architect

## Steps

1. Frame the decision, use case, benefit, scope and contract owner. Pin the revision and establish a baseline before claiming a gain.
2. Trace the [critical paths](references/analysis.md#explore-the-graph-and-find-critical-paths) to their consumers. Record shapes, owners, invariants and failure modes. Apply the [review angles](references/checks.md#review-angles) to the affected code. Match coverage to the workflow.
3. Give each finding a Hypothesis test. State the claim, strongest rival, distinguishing check and one result: **confirmed**, **dismissed** or **candidate**. Name the missing check for a candidate. Keep healthy invariants and disproof.
4. For review or an unproven seam, return findings and a plan. Before any authorized edit, apply the [gates](#gates) and [change discipline](references/change-discipline.md). Make the smallest change at the proven boundary with a focused test that detects the failure.
5. Verify affected wiring through the production path. Inspect exit status and payload errors. Compare before and after on the same measure. If a check fails or evidence goes stale, diagnose and retry when a useful authorized check remains. Report the gap when blocked or out of budget.

## When making a claim

- Compare [wiring views](references/analysis.md#model-the-wiring) and label rules declared, observed or inferred. Use [concepts](references/concepts.md) to distinguish a style, pattern, role or transport from its look-alikes. Report the observed mechanism and its consequence.
- For topology, when discovery lists `graphBuild`, run it on the directory, then `graphQuery` with the same `path` (`octocode schema graphQuery --view variants`); otherwise use `lspSearch` and `astSearch`. Edges suggest relationships. A type-only cycle needs separate runtime evidence.
- For caller counts or absence, union any graph results with `lspSearch callers` and check [coverage](references/analysis.md#choose-the-narrowest-evidence-lane) before claiming completeness. Either tool can miss callers.
- Match [evidence](references/evidence.md) to the claim. Distinguish IDs, paths, versions and roots in fixtures and assert the specific failure. Keep scope, flow, identity, runtime impact or measurement marked **candidate** while unresolved.

## Gates

- For public-contract rewires, cross-package moves, schema/storage migrations, deletes or renames, proceed only with exact authorization. Otherwise make a Decision request with options, tradeoffs, recommendation and default if unanswered.
- Clean what the change caused or touches. Broader refactoring requires evidence, an acceptance check and authorization.
- Update only repo-required bookkeeping and regenerate derived files. Make unimplemented reachable paths fail explicitly and test observable outcomes. Coordinate overlapping edits. Never stash, reset, overwrite or discard others’ work.

## Related skills

- For repo, GitHub, package or symbol facts and live schemas, use `octocode-research` and the [MCP/CLI guide](references/octocode-usage.md).
- To choose or coordinate a workflow, use `octocode-workflows` for [roles and handoffs](../octocode-workflows/references/composition.md#orchestrator-and-actor-contract) and [verification](../octocode-workflows/references/verification.md) for artifact identity, one coverage ledger and recovery. Load [whole-system traversal](../octocode-workflows/references/single-actor-traversal.md), [runtime surgery](../octocode-workflows/references/runtime-surgery.md) or [AST/LSP techniques](../octocode-workflows/references/ast-lsp-workflows.md) for that procedure. Use `octocode-agents-communication` for transport identities, leases or messages.
- Use `octocode-graph-view` for a saved graph brief or requested interactive view, `octocode-rfc-generator` for a reviewed decision document, and `octocode-clean-agentic-code` for behavior-preserving removal.

## Output

Follow [output.md](output.md), including its [decision record](output.md#decision-record) for consequential work. Use [render-report.mjs](scripts/render-report.mjs) for accepted HTML rendering or manifest discovery.
