# Cleanup Playbook

Load when starting the cleanup run or when the current phase is uncertain. Why: ordered phases enforce proof and consent gates before any deletion.

## SCOPE

State: target path(s), smell classes in scope (shims, re-exports, duplicates, docs, config, hierarchy, schemas, dependencies), and what is explicitly excluded.

## AUDIT — tool queries per class

| Class | Tool | Query |
|-------|------|-------|
| Shims / re-exports / aliases | `localSearch` text + `lspGetSemantics` callers + `localAnalyzeGraph` import edges | pattern match → zero-caller proof |
| Duplicate logic | `localSearch` structural (AST) | diff candidates; confirm consumers |
| Config length / redundancy | `localGetFileContent` minify:none | line count; key audit |
| Hierarchy / misplacement | `localSearch` tree | file count per folder; layer mismatch |
| Docs / comments | `localGetFileContent` minify:none | apply `references/doc-hygiene.md` rules |
| Schema / type redundancy | `localSearch` structural + `lspGetSemantics` callers | duplicate interfaces, type aliases, protocol stubs |
| Dependency junk | `localGetFileContent` minify:none on each package.json | unused, duplicate, misaligned, phantom deps |

## INVENTORY

Produce a table before TRIAGE:

| File | Line | Class | Confidence | Callers | Safe to delete? |
|------|------|-------|------------|---------|----------------|
| … | … | … | high/med/low | 0 / N | yes / no |

High = zero confirmed LSP callers + no graph import edge. Medium = zero LSP callers but graph edge exists. Low = uncertain; do not include in any batch.

## TRIAGE

Rank: safe deletions (high confidence) → prose-only config/doc trims → hierarchy moves → medium-confidence items needing further proof. Never include low-confidence items without additional verification.

## CONSENT

Present the inventory and proposed batch. State exactly what will be deleted or moved and which checks will run. Require explicit approval.

## EXCISE

1. Delete or inline confirmed dead code.
2. Update callers when a canonical replacement exists.
3. Move misplaced files and update imports.
4. Trim config and doc files per consent; keep each batch small enough to revert atomically.

## VERIFY

Run the project's own checks after each batch: `yarn build` → `yarn test` → `yarn typecheck` → `yarn lint`. Report exact output. Classify failures as pre-existing or introduced; revert introduced failures, never suppress them.

Next: after VERIFY passes, return to TRIAGE for the next batch or report done.
