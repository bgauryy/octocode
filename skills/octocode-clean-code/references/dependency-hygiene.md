# Dependency Hygiene

Load when auditing package.json files for unused, duplicate, misaligned, or phantom dependencies. Why: dependency junk inflates bundles, hides real conflicts, and creates compounding update debt.

## Unused dependencies

| Signal | Verification required |
|--------|----------------------|
| Package in `dependencies` / `devDependencies` with no import | `localSearch` text across all source files for the package name |
| Package used only in tests but listed in `dependencies` | Move to `devDependencies` after confirming no production import |
| Peer dep also listed as a direct dep | Confirm whether the package ships its own copy |

## Version misalignment (monorepo)

| Signal | Action |
|--------|--------|
| Same package at different versions across workspace packages | Choose the highest compatible version; update all declarations |
| Direct version pin duplicating a root `resolutions` entry | Remove pin; rely on the root resolution |
| Two packages pull the same transitive dep at different versions | Add an explicit root resolution to deduplicate |

## Duplicate declarations

| Signal | Action |
|--------|--------|
| Package declared in both root `package.json` and a workspace package | Keep only where the usage lives |
| Entry duplicated in both `dependencies` and `devDependencies` | Keep the correct category; remove the other |
| Build tool listed as both dev dep and peer dep | Choose one; confirm the consuming package’s intent |

## Phantom dependencies

| Signal | Action |
|--------|--------|
| Import used in source but absent from `package.json` | Add explicit declaration at the correct version |
| Import resolved only via a transitive dep with no direct declaration | Add direct declaration or remove the import |

## Workspace protocol hygiene

Monorepo internal packages must use `workspace:*` (or the project’s configured protocol) — never a version pin. Pinning an internal package breaks local resolution and causes install-time divergence.

## Consent gate

Removing a dependency or changing a version affects all consumers. Require explicit consent before any edit. Unused-only removals within an approved batch may proceed together.

Next: after consent, return to `references/cleanup-playbook.md` EXCISE phase.
