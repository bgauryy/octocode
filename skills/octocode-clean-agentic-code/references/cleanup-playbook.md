# Cleanup Playbook

Load when planning or applying cleanup. Select the evidence and checks for the proposed change; combine phases when the target and authority are already clear.

## SCOPE

State the target paths, which smell classes are in scope, and what is explicitly excluded.

## AUDIT — tool queries per class

| Class | Tool | Query |
|-------|------|-------|
| Shims / re-exports / aliases | `localSearch`, exact read, applicable `lspSearch` references, `astTopology` (beta) | discover candidates; trace symbol uses and configuration/entrypoint paths |
| Duplicate logic | `astSearch` match (AST) | diff candidates; confirm consumers |
| Config length / redundancy | `localFetch` minify:none | line count; key audit |
| Hierarchy / misplacement | `structureSearch` tree | file count per folder; layer mismatch |
| Docs / comments | `localFetch` minify:none | apply `references/doc-hygiene.md` rules |
| Schema / type redundancy | `astSearch` match + `lspSearch` references | compare shapes, semantic roles, consumers, and protocol compatibility |
| Dependency junk | `localFetch` minify:none on each package.json | unused, duplicate, misaligned, phantom deps |
| Test debt | `structureSearch` with `operation:"files"` and a name/path filter on the candidate package + `localFetch` with `minify:"symbols"` on each hit | numbered/dated files, skip blocks, rigid mocks, redundant stubs, env-coupled setup |
| Agent residue | `astTopology` dependents/deadCode (beta; else `lspSearch` references), then `localSearch` | zero-dependent new files, reinvented helpers, narration comments, masked failures |
| Spaghetti | `astSearch` on the function body, then an exact read; `lspSearch` callers for flag or mode arguments | phase-crossing and deep nesting inside one function; apply `references/spaghetti.md` |

## INVENTORY

Produce this table before TRIAGE:

| File | Line | Class | Confidence | Callers | Safe to delete? |
|------|------|-------|------------|---------|----------------|
| … | … | … | high/med/low | 0 / N | yes / no |

Assign confidence from completed evidence in the relevant scope. Missing edges or references alone are insufficient; account for entrypoints, dynamic consumers, public contracts, and configuration. Resolve material uncertainty before deleting the candidate.

## TRIAGE

Rank: safe deletions (high confidence) → prose-only config/doc trims → hierarchy moves → medium-confidence items needing further proof. Never include low-confidence items without additional verification. Route anything that disguises a failure to `references/agentic-correctness.md` instead of a batch. Route tangled control flow through `references/spaghetti.md` and keep that knot out of the excision batch.

## CONSENT

Keep the proposed deletions or moves and their checks reviewable. Apply a batch covered by existing authorization; ask only for missing scope or authority. A completed inventory does not create an extra approval requirement.

## EXCISE

1. Delete or inline confirmed dead code, updating callers when a canonical replacement exists.
2. Move misplaced files, update their imports, and trim config and doc files per consent — keeping each batch small enough to revert atomically.

## VERIFY

Run the relevant build, test, typecheck, and lint commands required by the repository. Inspect their actual output; repair introduced failures and distinguish pre-existing failures. Preserve required coverage floors and acceptance checks. If deleting a test removes useful coverage, restore that behavior check as described in `references/test-quality.md`; do not relax a threshold to make the cleanup pass.

Next: after VERIFY passes, return to TRIAGE for the next batch or report done.
