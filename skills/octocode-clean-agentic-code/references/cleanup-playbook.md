# Cleanup Playbook

Load to plan or apply a batch. This page holds the AUDIT queries and EXCISE detail.

## AUDIT queries

| Class | Query |
|---|---|
| Shims, re-exports, aliases | `localSearch` candidates; exact read; `lspSearch` references; `astTopology` (CLI beta) for config and entrypoint paths |
| Duplicate logic | `astSearch` match; diff candidates; confirm consumers |
| Config length, redundancy | `localFetch` minify:none; line count; key audit |
| Hierarchy, misplacement | `structureSearch` tree; files per folder; layer mismatch |
| Docs, comments | `localFetch` minify:none; `references/doc-config-hygiene.md` |
| Schema, type redundancy | `astSearch` match + `lspSearch` references; compare shapes, roles, consumers, protocol compatibility |
| Dependency junk | `localFetch` minify:none on each package.json |
| Test debt | `structureSearch` `operation:"files"` with a name/path filter, then `localFetch` `minify:"symbols"` per hit |
| Agent residue, bloat | `astTopology` dependents/deadCode (CLI beta; else `lspSearch` references), then `localSearch` the signals in `references/agentic-bloat.md` and `references/test-gaming.md` |
| Instruction cruft | `localSearch` the `references/instruction-cruft.md` signals over the inventory; `localFetch` each hit; `git blame` for provenance |
| Spaghetti | `astSearch` the body, exact read, `lspSearch` callers for flag or mode arguments (`references/structure.md` § Spaghetti) |

## EXCISE detail
- Delete or inline confirmed dead code.
- Move misplaced files, then update imports; trim config and docs within consent.
- Restore coverage with `references/test-hygiene.md` § Replacement tests.
