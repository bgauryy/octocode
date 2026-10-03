# Declaration hygiene: types, schemas, dependencies

Load when reviewing type definitions, interfaces, enums, schemas, or protocol shapes for redundancy or aliasing, or package.json files for unused, duplicate, misaligned, or phantom dependencies.

## Types and schemas

| Signal | Verification or action |
|---|---|
| `type Foo = Bar` with no added constraint | LSP: all consumers can reference Bar directly |
| `interface A extends B {}` with no extra members | Trace A's references and intended role; matching members alone do not establish equivalence |
| `enum X` duplicating another enum's values | Identify the canonical enum; confirm all callers on it |
| Two interfaces with identical or near-identical shape in different modules | AST structural match; one is a verbatim copy |
| Same Zod / JSON Schema object defined in two files | Text search for distinguishing field literals; choose the canonical one |
| Protocol type re-declared per version (`v1`, `v2`) with identical shape | Trace supported producers and consumers; check persisted and serialized payload compatibility before sharing a definition |
| Compatibility shim mapping old message format to new | All producers emit the new format; the shim is dead |
| Type reachable only behind an always-on feature flag | Inline unconditionally; remove the flag branch |

Use exact source and `astSearch` to compare constraints, then `lspSearch` references for consumers in the configured project. Type aliases and interfaces have no callable hierarchy. Check public exports, generated consumers, and protocol/version boundaries; empty references alone do not establish safe deletion. Do not merge types that share shape but serve different semantic roles (`UserId` and `ProductId` as `string` aliases): confirm semantic identity before deletion.

## Dependencies

| Signal | Verification or action |
|---|---|
| Package in `dependencies` / `devDependencies` with no import | `localSearch` text across all source files for the package name |
| Package used only in tests but listed in `dependencies` | Move to `devDependencies` after confirming no production import |
| Peer dep also listed as a direct dep | Confirm whether the package ships its own copy |
| Same package at different versions across workspace packages | Choose the highest compatible version; update all declarations |
| Direct version pin duplicating a root `resolutions` entry | Remove the pin; rely on the root resolution |
| Two packages pull the same transitive dep at different versions | Add an explicit root resolution |
| Package in root `package.json` and a workspace package | Keep it only where the usage lives |
| Entry in both `dependencies` and `devDependencies` | Keep the correct category |
| Build tool listed as both dev dep and peer dep | Choose one; confirm the consuming package's intent |
| Import in source but absent from `package.json`, or resolved only through a transitive dep | Add a direct declaration at the correct version, or remove the import |

Internal monorepo packages use `workspace:*` (or the project's configured protocol), never a version pin; a pin breaks local resolution.

Consent gate: removing a dependency or changing a version affects all consumers and needs explicit consent before any edit. Unused-only removals within an approved batch may proceed together.

Next: return to `references/cleanup-playbook.md` TRIAGE and EXCISE.
