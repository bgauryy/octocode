# Change discipline: refactor and ship a slice

Load before any source edit: when a proven architecture or algorithm finding needs a code change, the user asks to improve the design, or Code or Review is shipping a slice. A plausible target diagram is not a safe migration; a slice is incomplete until its behavior and repository state agree.

## Refactor

Do not refactor a candidate whose impact or acceptance check is undefined.

| Finding | Small safe move | Guardrail |
|---|---|---|
| Mixed responsibility | extract one cohesive capability behind the existing call shape | characterization and unchanged public behavior |
| Wrong-way dependency | introduce a consumer-owned port; move infrastructure to an adapter | contract test and exact negative dependency check |
| Leaky transport/storage/view type | map to a boundary value or DTO at the edge | serialization, validation, and compatibility tests |
| Hidden orchestration/effects | expose the use-case boundary and effect owner | success, error, retry, timeout, cancellation |
| Repeated policy | centralize only proven shared knowledge/change pressure | preserve genuinely divergent edge cases |
| Over-wide interface | derive focused capabilities from real consumer groups | no capability loss or one-interface-per-call ceremony |
| Measured repeated work | fix I/O/computation or cache/lifetime ownership | same benchmark conditions plus correctness/resource limits |
| Harmful runtime cycle | break the least stable edge through inversion, events, or responsibility movement | initialization/integration checks; ignore type-only cycles |

Prefer boundaries around capability, volatility, and ownership—not arbitrary technical nouns. Point dependencies toward stable policy, keep construction visible at a small number of composition roots, and make transactions, retries, cancellation, failure, and observability ownership explicit. Duplicate simple syntax before abstracting unrelated concepts.

Migration: freeze behavior → create seam → move one scenario → redirect callers → verify architecture and runtime sensors → repeat → remove old paths only after semantic and external-consumer checks. Prefer reversible vertical slices over layer-by-layer rewrites; update architecture docs when they are part of the contract. Return to `references/algorithm-review.md` or `references/architecture-analysis.md` when the seam exposes an unproven assumption.

## Verify the slice

- Verify that the first failing assertion fails for the intended reason.
- Do not stub the dependency whose integration or behavior the test claims to prove.
- Run the narrowest relevant checks, then expand according to affected scope; compare results with the recorded baseline.
- Passing assertion text inside a non-zero process is failed verification.
- Clean up acquired resources; preserve generated-code ownership.
- For optimization, record the metric.

## Cleanup and bookkeeping

Leave touched territory clean: remove dead imports and obsolete code created or exposed by the change; fix affected formatting, stale comments, naming drift, and minor structural clutter when the cleanup is safe. If cleanup changes public behavior, ownership, architecture, or meaningful review scope, report it as separate work instead of smuggling it into the slice.

Discover repository conventions rather than updating every possible record. Avoid blanket version bumps, lockfile churn, snapshot acceptance, or changelog entries without a repository-specific reason.

## Definition of done

The owned interface behaves as intended on normal and named edge paths, and every remaining failure carries a class and attribution evidence.

Next: use `references/output-contracts.md` to report a consequential result; otherwise return a concise outcome and verification summary.
