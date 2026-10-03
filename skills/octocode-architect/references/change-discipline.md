# Change discipline: refactor and ship a slice

Load before any source edit: when a proven architecture or algorithm finding needs a code change, the user asks to improve the design, or Code or Review is shipping a slice. A plausible target diagram is not a safe migration; a slice is incomplete until its behavior and repository state agree.

## Refactor

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

Migration: freeze behavior → create seam → move one scenario → redirect callers → verify architecture and runtime sensors → repeat → remove old paths only after semantic and external-consumer checks. Prefer reversible vertical slices over layer-by-layer rewrites; update architecture docs when they are part of the contract. Return to `references/algorithm-review.md` or `references/architecture-analysis.md` when the seam exposes an unproven assumption.

## Verify the slice

- Start with a failing assertion on the owned interface; verify that it fails for the intended reason.
- Exercise the production path. Do not stub the dependency whose integration or behavior the test claims to prove.
- Run the narrowest relevant checks, then expand according to affected scope. Compare results with the recorded baseline, and inspect the final diff.
- The command exit status controls green; passing assertion text inside a non-zero process is failed verification. A zero exit with an error status in the payload is also a failure.
- Fail reachable unfinished paths explicitly; clean up acquired resources; preserve generated-code ownership.
- For optimization, record the metric, and baseline, change one variable, rerun comparably, and keep only measured improvement.

## Cleanup and bookkeeping

Leave touched territory clean: remove dead imports and obsolete code created or exposed by the change; fix affected formatting, stale comments, naming drift, and minor structural clutter when the cleanup is safe and directly related. Do not broaden the task into a refactor. If cleanup changes public behavior, ownership, architecture, or meaningful review scope, report it as separate work instead of smuggling it into the slice.

Discover repository conventions rather than updating every possible record. When the implementation or release contract requires it, update the authoritative changelog, version, documentation, manifest, schema, generated file, lockfile, fixture, or snapshot. Regenerate derived artifacts from their source; never hand-edit generated output. Avoid blanket version bumps, lockfile churn, snapshot acceptance, or changelog entries without a repository-specific reason.

## Definition of done

1. The owned interface behaves as intended on normal and named edge paths.
2. Impacted callers, data paths, operations, and rollback assumptions were checked. <!-- style-lint: ignore-line passive-voice -->
3. Relevant tests and sensors passed, or remaining failures are classified and reported with attribution evidence.
4. Task-scoped cleanup is complete and unrelated cleanup is excluded. <!-- style-lint: ignore-line passive-voice -->
5. Required bookkeeping matches the code; no stale derived or descriptive state remains.

Next: use `references/output-contracts.md` to report a consequential result; otherwise return a concise outcome and verification summary.
