# Smell Catalog

Load when classifying a target as dead, duplicate, kludge, or junk prose.

## Re-exports and barrel aliases

| Signal | Verification required |
|--------|----------------------|
| `export { X } from './X'` with no added logic | Trace references, import paths, package exports, and external API obligations |
| `export * from './module'` barrel that only re-namespaces | Inspect import candidates and public entrypoints; graph absence cannot exclude external consumers |
| `export default aliasedName` wrapping another export | Compare alias/original references and observable export behavior |

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
| TODO with no ticket or owner | Confirm it is obsolete or captured elsewhere; age alone does not establish that |
| `@deprecated` JSDoc with no migration path | After caller updates complete |

Next: when the class is confirmed, load `references/cleanup-playbook.md` for the TRIAGE and EXCISE phases. Test smells have their own evidence bar in `references/test-hygiene.md`. When the code was agent-authored, load `references/agentic-defects.md` for signatures this catalog does not cover.
