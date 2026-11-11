# Output

Follow the [shared presentation and HTML delivery rules](references/presentation.md); retain this skill’s artifact format.

## Response

Lead with always-loaded totals against budgets and the `high` finding count. For each high finding, report:

| Slot | Include |
|---|---|
| What / where | Flag; file, skill or server |
| When / who | Always loaded, startup or on call; every session or one client |
| Why / how | Measured cause; fix from “Act on the report” |
| How much | Characters, approximate tokens or error rate; label estimates |

Use `unknown` for unavailable evidence. A rerun gives totals before/after and fixes kept/reverted as a Delta update.

## Saved result

One report in a fresh dated directory under `<workspace>/.octocode/octocode-context-audit/` (the script's default `--out`; a re-run for comparison takes another `--out`). `context-audit.json` is the saved evidence. After the task, offer HTML; when accepted, run `node <skill>/scripts/context-audit.mjs --render <saved-context-audit.json>` to create its sibling `context-audit.html` without collecting again. Use `--html` during the audit only when HTML was already requested. Do not add a third report file.

Protocol definitions: [shared catalog](references/protocol-catalog.md#catalog).
