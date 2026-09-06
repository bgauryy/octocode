# Hierarchy Rules

Load when evaluating file placement, folder cohesion, or size limits. Why: detect god files, god folders, and misplaced files before moving anything.

## God file

A file is a god file when it exceeds **400 LOC** AND owns more than one conceptual responsibility. Size alone is not the signal — a 600-line pure-data file is fine; a 200-line file doing IO + parsing + formatting is a god file.

Split protocol:
1. Name each responsibility.
2. Identify the owning layer (schema, transport, util, domain, config, …).
3. Create one file per responsibility under its layer directory.
4. Update all callers; run build + tests.

## God folder

A folder is a god folder when it holds **more than 20 files** spanning more than one domain, or when file names require a prefix to distinguish families (`user-api.ts`, `user-db.ts`, `product-api.ts` all flat in one directory).

Split protocol: create sub-directories per domain; move files; update relative imports.

## Misplaced files

| Signal | Correct location |
|--------|----------------|
| `utils/auth-logic.ts` — business logic in util | `domain/auth/` or `services/auth/` |
| `config/user-model.ts` — model in config | `models/` or `domain/` |
| `api/db-queries.ts` — persistence in API layer | `db/` or `repositories/` |

Move, update imports, verify with build + LSP. Do not rename unless the name is also wrong.

## God documentation

A doc is a god doc when it covers more than one concept, exceeds 300 lines, or is the sole entry point for a domain that deserves multiple focused docs. Split into one-concept files linked from a lean index kept under 50 lines.

Next: after moves are complete, return to `references/cleanup-playbook.md` VERIFY phase; for prose and comment concerns load `references/doc-hygiene.md`.
