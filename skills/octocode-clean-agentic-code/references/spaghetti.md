# Spaghetti code

Load when a cleanup target or a proposed edit has tangled control flow, or when an excision would add a branch, flag, wrapper, or copy to keep a knot alive. Why: detect spaghetti and refuse to write or extend it.

Spaghetti is live code whose next step depends on flags, nested special cases, or jumps across unrelated phases. A long straight procedure is not spaghetti. A short function whose phases cross is. File size and multiple responsibilities stay in `references/hierarchy-rules.md`.

## Detect

Read the function. Use `astSearch` for its branch, loop, and try shape, then `lspSearch` callers to see which flags or modes are actually passed. File length, a single `if`, or god-file size does not establish spaghetti. Confidence is high only after the body was read.

| Signal | What it shows |
|--------|----------------|
| Conditionals, loops, or try/catch nested more than three levels, and an inner level decides a different job than the outer level | Phases are fused |
| A boolean, mode, status, or enum is set in one region and read in a distant branch of the same function | Control is implicit |
| The same decision is tested again after an earlier branch already settled it | Fall-through undoes a settled path |
| An exception, labeled break, or callback is how the function moves between phases | The jump is the structure |
| The cleanup typechecks only after a new flag, wrapper, nested condition, or copied function | The edit is weaving |

## Avoid

Two moves stay in an excision batch, and observable behavior stays the same: delete a branch that callers, entrypoints, and config show is unreachable, and move one already-straight phase to its existing owner without adding a branch.

Every other knot stays out of the batch. Inventory it as class `spaghetti`, set Safe to delete to no, and record the file, line range, and signals. Hand a structural untangle to `octocode-architect`.

Next: return to `references/cleanup-playbook.md` TRIAGE. God-file splits stay in `references/hierarchy-rules.md`.
