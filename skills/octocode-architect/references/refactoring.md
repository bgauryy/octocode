# Architecture Refactoring

Load when a proven architecture or algorithm finding needs a code change, or the user explicitly asks to improve the design. Why: a plausible target diagram is not a safe migration.

Write `proven problem → harmed quality attribute → proposed seam → preserved/changed contract → vertical slice → sensor`. Do not refactor a candidate whose impact or acceptance check is undefined.

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

Migration: freeze behavior → create seam → move one scenario → redirect callers → verify architecture and runtime sensors → repeat → remove old paths only after semantic and external-consumer checks. Prefer reversible vertical slices over layer-by-layer rewrites; update architecture docs when they are part of the contract.

Return to `algorithm-review.md` or `architecture-analysis.md` when the seam exposes an unproven assumption, then use `delivery-discipline.md` to ship the slice.
