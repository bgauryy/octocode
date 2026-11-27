Load when a candidate is a redundant type or schema declaration, or a declared, undeclared, duplicated or misplaced dependency.

# Types and dependencies

## Types and schemas

- Consumers: `lspSearch` references (aliases, interfaces lack call hierarchy); constraints: exact read + `astSearch`. Empty references: still check public exports, generated consumers, protocol and version boundaries.
- Same shape ≠ same role: merge only on proven semantic identity (`UserId`, `ProductId` as `string` stay apart).

| Signal | Check / action |
|---|---|
| Constraint-free `type Foo = Bar`, empty `interface A extends B {}` | LSP shows every consumer can use `Bar`; trace A's role. |
| Duplicate `enum`, near-identical interfaces, same Zod or JSON Schema object twice | AST match or distinguishing field literals; pick canonical; confirm callers. |
| Same-shape per-version protocol types (`v1`, `v2`), old-to-new message shim | all producers emit the new shape; check persisted and serialized compatibility first. |
| Type behind an always-on flag | inline it; drop the branch. |

## Dependencies

- Removal or version change: explicit consent, except unused-only removals in an approved batch.
- Internal monorepo package: `workspace:*` (or configured protocol), never a version pin.

| Signal | Check / action |
|---|---|
| Declared, never imported; imported but undeclared | For a JS/TS package run [deps-usage](../scripts/deps-usage.mjs): `node <skill-folder>/scripts/deps-usage.mjs <package-dir>`. `unimported` is a removal candidate once no tool, plugin shorthand or external consumer loads it; `referencedOnly` is used by scripts or config: keep. `undeclared` (with `declaredBy` when only an ancestor manifest declares it) → declare at the correct version, or drop the import. Other ecosystems or a whole workspace: graphQuery `issues` with `detectors: ["undeclared-dependency"]` (imported but undeclared, hoisted phantoms included). |
| Test-only in `dependencies` (deps-usage `testOnlyInDependencies`) | `devDependencies` once no production import. Peer also direct (`inMultipleSections`) → does the package ship its own copy? |
| Versions differ across workspace packages | inspect consumer ranges and compatibility before deduplicating. Multiple transitive versions may be required; use a root resolution only after checking every affected consumer. |
| In root and workspace `package.json` | keep where used; in `dependencies` and `devDependencies` (`inMultipleSections`) → keep the correct one; build tool as dev and peer → per consumer intent. |
