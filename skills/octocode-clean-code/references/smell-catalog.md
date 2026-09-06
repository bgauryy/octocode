# Smell Catalog

Load when classifying a target as dead, duplicate, kludge, or junk prose. Why: class determines the evidence bar and excision protocol.

## Re-exports and barrel aliases

| Signal | Verification required |
|--------|----------------------|
| `export { X } from './X'` with no added logic | LSP zero-callers on re-export path + graph zero-import-edges |
| `export * from './module'` barrel that only re-namespaces | Graph: no external consumer of the barrel |
| `export default aliasedName` wrapping another export | LSP callers on alias and original both checked |

## Legacy shims and compatibility stubs

| Signal | Verification required |
|--------|----------------------|
| Comment: `// legacy`, `// compat`, `// deprecated`, `// removed in vX` | No caller that cannot use the real path |
| Adapter function mapping old API shape to new | All call sites confirmed on the new shape |
| `if (legacyMode)` / `if (version < X)` conditionals | Branch never true in any live config |

## Duplicate logic

| Signal | Verification required |
|--------|----------------------|
| Near-identical function bodies in different modules | AST structural match; diff the two bodies |
| Copy-pasted constant blocks | Text search for the literal; confirm all sites |
| Parallel helpers imported by the same consumers | Graph: both edges confirmed; canonical chosen |

Keep the canonical copy; update all callers before deleting the duplicate.

## Patch kludges and regex fixups

| Signal | Verification required |
|--------|----------------------|
| `str.replace(/old-value/, …)` at module scope | Value correctable at its source |
| `Object.assign(prototype, …)` outside tests | Patched object is internal and owned |
| Env check always true in deployed config | Confirmed across all deployment targets |

## Junk prose

| Type | Remove when |
|------|------------|
| Syntax narration (`// increment counter`) | Always |
| Dead comment block (`/* old impl */`) | Always |
| TODO with no ticket and no owner >90 days | Always |
| `@deprecated` JSDoc with no migration path | After caller updates complete |

Next: when the class is confirmed, load `references/cleanup-playbook.md` for the TRIAGE and EXCISE phases.
