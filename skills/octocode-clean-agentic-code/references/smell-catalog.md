# Smell Catalog

Load to classify a target as dead, duplicate, or kludge. Comments: [text and config hygiene](text-config-hygiene.md). Tests: `references/test-hygiene.md`. Agent-authored code: `references/agentic-defects.md`.

| Class | Signal | Delete only when |
|---|---|---|
| Re-export | `export { X } from './X'`, no added logic | References, import paths, package exports, and external API obligations are traced |
| Barrel | `export * from './module'` that only re-namespaces | Imports and public entrypoints are inspected; graph absence cannot exclude external consumers |
| Alias | `export default aliasedName` wrapping another export | Both references and the observable export behavior match |
| Legacy shim | `// legacy`, `// compat`, `// deprecated`, `// removed in vX` | No caller needs it to reach the real path |
| Adapter | Maps an old API shape to the new | All call sites use the new shape |
| Version branch | `if (legacyMode)` / `if (version < X)` | Never true in any live config |
| Duplicate body | Near-identical bodies in two modules | AST structural match; bodies diffed |
| Duplicate constants | Copy-pasted constant blocks | A text search for the literal confirms all sites |
| Parallel helpers | Same consumers import two helpers | The graph confirms both edges; a canonical helper is chosen |
| Regex fixup | `str.replace(/old-value/, …)` at module scope | The value is correctable at its source |
| Prototype patch | `Object.assign(prototype, …)` outside tests | The patched object is internal and owned |
| Dead env check | Always true in deployed config | Confirmed on all deployment targets |

Next: the [lobby workflow](../SKILL.md#workflow).
