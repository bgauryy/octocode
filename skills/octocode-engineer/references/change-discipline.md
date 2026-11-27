Load when about to edit source: a proven fix, a requested design improvement, or an agent's slice. A slice is done only when behavior and repository state agree.

Read next: [removal and unification](change-removal.md) before deleting code or merging duplicated policy.

# Change discipline

## Gates

- Keep public-contract rewires, cross-package moves, schema/storage migrations, deletes and renames within existing authorization. A review alone authorizes findings, not those effects. Resolve only the missing decision with options, trade-offs and a recommendation; silence is not approval.
- Cleanup stops at what the change caused or touched ([verify the slice](#verify-the-slice)). Broader refactoring needs evidence, an acceptance check and authorization.
- Update only required bookkeeping; regenerate derived files. Reachable unimplemented paths must fail explicitly. Coordinate overlapping edits; never stash, reset, overwrite or discard others' work.

## Refactor

- No refactor while a candidate's impact or acceptance check is undefined.

Safe moves (finding → move; guardrail):

| Finding | Action / proof |
|---|---|
| Mixed responsibility | extract one cohesive capability behind the existing call shape; characterization tests. |
| Wrong-way dependency | consumer-owned port, infrastructure in an adapter; contract test, negative dependency check. |
| Leaky transport/storage/view type | boundary value or DTO at the edge; serialization, validation, compatibility tests. |
| Hidden orchestration or effects | expose the use-case boundary and effect owner; success, error, retry, timeout, cancellation. |
| Repeated policy | centralize only proven shared knowledge and change pressure; keep divergent edge cases. |
| Over-wide interface | focused capabilities from real consumer groups; no capability loss. |
| Measured repeated work | fix I/O, computation, or cache/lifetime ownership; same benchmark conditions and limits. |
| Harmful runtime cycle (not type-only) | break the least stable edge (inversion, events, moved responsibility); initialization and integration checks. |

- Boundaries: cut by capability, volatility, and ownership, not technical nouns; dependencies point toward stable policy; construction stays at few composition roots; transactions, retries, cancellation, failure, and observability each get an owner. Prefer duplicated simple syntax over abstracting unrelated concepts.
- Migrate in reversible vertical slices: freeze behavior, create a seam, move one scenario, redirect callers, then run architecture and runtime checks. Repeat for the remaining scenarios. Remove old paths only after semantic and external-consumer checks; update architecture docs that are part of the contract.
- A seam exposing an unproven assumption: back to [algorithm checks](checks-algorithms-resources.md#algorithms) or [analysis](analysis.md).

## Verify the slice

- For runtime checks, verify loaded artifact identity, record coverage and diagnose failures before another change. A selected `octocode-workflows` procedure, when installed, supplies detailed verification and recovery.
- The first failing assertion fails for the intended reason.
- Never stub the dependency the test claims to prove.
- Run the narrowest checks, widen with affected scope, compare with the baseline.
- Passing assertion text in a non-zero process is a failure.
- Release acquired resources; preserve generated-code ownership.
- Optimization: record the metric.
- Remove dead imports and obsolete code the change created or exposed; fix affected formatting, stale comments, naming drift. Broader cleanup and bookkeeping: [gates](#gates).

## Done

Done when the owned interface behaves as intended on normal and relevant edge paths. Explain remaining failures or unverified scope with the evidence available. Record the result per [decision record](../output.md#decision-record).
