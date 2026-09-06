# Schema Hygiene

Load when reviewing type definitions, interfaces, enums, schemas, or protocol shapes for redundancy or aliasing. Why: duplicate type surface fractures contracts and makes coordinated changes expensive.

## Type alias redundancy

| Signal | Verification required |
|--------|----------------------|
| `type Foo = Bar` with no added constraint | LSP: all consumers can reference Bar directly |
| `interface A extends B {}` with no extra members | LSP: A has zero callers independent of B |
| `enum X` duplicating another enum's values | Canonical enum identified; all callers confirmed on it |

Confidence required: LSP zero-callers on the alias AND structural proof it adds no constraint.

## Duplicate interface and schema definitions

| Signal | Verification required |
|--------|----------------------|
| Two interfaces with identical or near-identical shape in different modules | AST structural match; one is a verbatim copy |
| Same Zod / JSON Schema object defined in two files | Text search for distinguishing field literals; canonical chosen |
| Protocol message type re-declared per version with identical shape | Confirm no consumer is version-pinned; old copy zero-callers |

## Protocol and versioned stubs

| Signal | Action |
|--------|--------|
| `v1`, `v2` protocol type still in scope | LSP zero-callers; delete after confirming no serialized payload reads it |
| Compatibility shim mapping old message format to new | All producers emit new format; shim dead |
| Feature-flag-gated type reachable only when flag is always-on | Inline unconditionally; remove flag branch |

## What not to merge

Do not merge types that share shape but serve different semantic roles (`UserId` and `ProductId` as `string` aliases). Structural match is necessary but not sufficient — confirm semantic identity before deletion.

Next: when the redundant type is confirmed dead, load `references/cleanup-playbook.md` TRIAGE and EXCISE phases.
