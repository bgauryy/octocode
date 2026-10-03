# Structure: hierarchy and spaghetti

Load when evaluating file placement, folder cohesion, size limits, or tangled control flow, or when an excision would add a branch, flag, wrapper, or copy to keep a knot alive.

## God file, god folder, god doc

| Smell | Signal | Split protocol |
|---|---|---|
| God file | Over **400 LOC** AND more than one conceptual responsibility. A 600-line pure-data file is fine; a 200-line file doing IO + parsing + formatting is a god file. | Name each responsibility and its owning layer (schema, transport, util, domain, config, …); create one file per responsibility under that layer; update all callers; run build + tests. |
| God folder | **More than 20 files** spanning more than one domain, or names that need a prefix to separate families (`user-api.ts`, `user-db.ts`, `product-api.ts` flat in one directory) | Create sub-directories per domain; move files; update relative imports. |
| God doc | Covers more than one concept, exceeds 300 lines, or is the sole entry point for a domain that deserves focused docs | Split into one-concept files linked from a lean index under 50 lines. |

## Misplaced files

| Signal | Correct location |
|---|---|
| `utils/auth-logic.ts`: business logic in util | `domain/auth/` or `services/auth/` |
| `config/user-model.ts`: model in config | `models/` or `domain/` |
| `api/db-queries.ts`: persistence in API layer | `db/` or `repositories/` |

Move, update imports, verify with build + LSP. Do not rename unless the name is also wrong.

## Spaghetti

Spaghetti is live code whose next step depends on flags, nested special cases, or jumps across unrelated phases. A long straight procedure is not spaghetti. A short function whose phases cross is. Detect spaghetti and refuse to write or extend it.

Read the function. Use `astSearch` for its branch, loop, and try shape, then `lspSearch` callers to see which flags or modes callers pass. File length, a single `if`, or god-file size does not establish spaghetti. Confidence is high only after you read the body.

| Signal | What it shows |
|---|---|
| Conditionals, loops, or try/catch nested more than three levels, and an inner level decides a different job than the outer level | Phases are fused |
| A boolean, mode, status, or enum is set in one region and read in a distant branch of the same function | Control is implicit |
| The same decision is tested again after an earlier branch settled it | Fall-through undoes a settled path |
| An exception, labeled break, or callback moves the function between phases | The jump is the structure |
| The cleanup typechecks only after a new flag, wrapper, nested condition, or copied function | The edit is weaving |

Only two spaghetti moves stay in an excision batch, with the same observable behavior: delete a branch that callers, entrypoints, and config show is unreachable; move one already-straight phase to its existing owner without adding a branch. Every other knot stays out of the batch: inventory it as class `spaghetti`, set Safe to delete to no, record the file, line range, and signals, and hand a structural untangle to `octocode-architect`.

Next: return to `references/cleanup-playbook.md` TRIAGE, or VERIFY after moves; for prose concerns load `references/doc-config-hygiene.md`.
