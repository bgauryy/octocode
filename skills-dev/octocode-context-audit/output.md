# Output

Follow the [shared presentation and HTML delivery rules](references/presentation.md); retain this skill’s artifact format.

## Response

BLUF in chat: always-loaded totals against their budgets and the count of `high` findings first, then each `high` finding in 5W2H: what (the flag), where (file, skill or server), when (always loaded, on start, on call), who it affects (every session, one client), why (the measured cause), how (the fix from "Act on the report"), how much (chars, ≈ tokens, or error rate). Write "unknown" in a slot the report does not show. A re-run reports a Delta update: totals before and after, and each fix kept or reverted.

## Saved result

One report in a fresh dated directory under `<workspace>/.octocode/octocode-context-audit/` (the script's default `--out`; a re-run for comparison takes another `--out`). `context-audit.json` is the saved evidence. After the task, offer HTML; when accepted, run `node <skill>/scripts/context-audit.mjs --render <saved-context-audit.json>` to create its sibling `context-audit.html` without collecting again. Use `--html` during the audit only when HTML was already requested. Do not add a third report file.

Protocol definitions: [shared catalog](references/protocol-catalog.md#catalog).
