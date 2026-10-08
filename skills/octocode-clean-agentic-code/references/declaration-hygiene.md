# Declaration hygiene: types, schemas, dependencies

Load for redundant or aliased types, interfaces, enums, schemas, or protocol shapes, and for unused, duplicate, misaligned, or phantom dependencies in package.json.

## Types and schemas

| Signal | Verify or act |
|---|---|
| `type Foo = Bar` with no added constraint | LSP: all consumers can use Bar |
| `interface A extends B {}` with no members | Trace A's references and role; same members do not prove equivalence |
| `enum X` duplicating another enum | Pick the canonical enum; confirm callers |
| Near-identical interfaces in two modules | AST structural match shows a copy |
| Same Zod / JSON Schema object in two files | Text-search distinguishing field literals; pick one |
| Protocol type re-declared per version (`v1`, `v2`), same shape | Trace producers and consumers; check persisted and serialized payload compatibility first |
| Shim mapping old message format to new | All producers emit the new format |
| Type reachable only behind an always-on flag | Inline it; remove the flag branch |

- Compare constraints with an exact read and `astSearch`; find consumers with `lspSearch` references. Aliases and interfaces have no call hierarchy.
- Check public exports, generated consumers, and protocol/version boundaries; empty references do not prove a delete safe.
- Do not merge same-shape types with different roles (`UserId` and `ProductId` as `string`). Confirm semantic identity first.

## Dependencies

| Signal | Act |
|---|---|
| In `dependencies` / `devDependencies`, no import | `localSearch` the package name across all sources |
| Used only in tests, listed in `dependencies` | Move to `devDependencies` after no production import is confirmed |
| Peer dep also a direct dep | Check whether the package ships its own copy |
| Different versions across workspace packages | Highest compatible version everywhere |
| Pin duplicating a root `resolutions` entry | Remove the pin |
| One transitive dep at two versions | Add a root resolution |
| In root and in a workspace `package.json` | Keep it where the usage lives |
| In both `dependencies` and `devDependencies` | Keep the correct one |
| Build tool as both dev and peer dep | Choose per the consumer's intent |
| Imported but undeclared, or resolved only transitively | Declare it at the correct version, or remove the import |

- Internal monorepo packages use `workspace:*` (or the configured protocol), never a pin; a pin breaks local resolution.
- A dependency removal or version change needs explicit consent first. Unused-only removals in an approved batch may proceed.

Next: run the batch with [lobby workflow](../SKILL.md#workflow).
