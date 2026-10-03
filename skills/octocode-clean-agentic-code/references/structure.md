# Structure: hierarchy and spaghetti

Load for file placement, folder cohesion, size limits, or tangled control flow, or when an excision would add a branch, flag, wrapper, or copy to keep a knot alive.

| Smell | Signal | Split |
|---|---|---|
| God file | A 600-line pure-data file is fine; a 200-line file with IO + parsing + formatting is not | One file per responsibility under its owning layer (schema, transport, util, domain, config); update callers; build + tests |
| God folder | Prefixes that separate families (`user-api.ts`, `user-db.ts`, `product-api.ts` flat) | One sub-directory per domain; move; update relative imports |
| God doc | The sole entry for a domain | One-concept files linked from an index under 50 lines |

| Misplaced | Move to |
|---|---|
| `utils/auth-logic.ts` (business logic in util) | `domain/auth/` or `services/auth/` |
| `config/user-model.ts` (model in config) | `models/` or `domain/` |
| `api/db-queries.ts` (persistence in API) | `db/` or `repositories/` |

Move, update imports, verify with build + LSP. Rename only a wrong name.

## Spaghetti

Spaghetti is live code whose next step depends on flags, nested special cases, or jumps across unrelated phases. A long straight procedure is not spaghetti; a short function whose phases cross is.

Read the body, `astSearch` its branch, loop, and try shape, and `lspSearch` callers for passed flags or modes. Length, one `if`, or god-file size proves nothing. Confidence is high only after you read the body.

| Signal | Shows |
|---|---|
| Conditionals, loops, or try/catch nested over three levels, an inner level deciding a different job | Fused phases |
| A boolean, mode, status, or enum set in one region and read in a distant branch | Implicit control |
| A decision tested again after a branch settled it | Fall-through undoes a settled path |
| An exception, labeled break, or callback that moves between phases | The jump is the structure |
| A cleanup that typechecks only after a new flag, wrapper, nested condition, or copied function | The edit is weaving |

Prove a branch unreachable through callers, entrypoints, and config; move a phase without a new branch. Inventory every other knot as class `spaghetti`, Safe to delete = no, with file, line range, and signals. Next: `references/cleanup-playbook.md` TRIAGE.
