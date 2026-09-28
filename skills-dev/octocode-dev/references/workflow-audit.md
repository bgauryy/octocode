# Workflow audit

Load when judging whether an agent will chain this tool into a smart research workflow. Why: a correct tool used at the wrong moment, or a dead end with no next step, wastes more than a bug.

## Checks

- **Next step is explicit.** Every result state (hits, zero hits, too many hits, error, partial) returns either an executable `next.*` call or a diagnostic naming the correction. Zero-result and invalid-input paths matter most.
- **Hints are specific and earned.** A hint names the tool + fields to call next with values from this result (path, line, SHA, symbol). Flag generic advice, hints repeated on every row, and hints pointing to disabled tools.
- **Cross-tool handoffs line up.** Producer fields feed consumer inputs without reshaping: search → fetch (`path` + line/anchor), ghSearchCode/ghStructure → ghGetFileContent (owner/repo/ref), history → item (number/SHA), astSearch topology → lspSearch confirmation. Check against `docs/TOOL_DATA_CONTRACT.md` "Connections between tools".
- **Routing text matches behavior.** Instructions' locate cascade (anchor → search → fetch → rerank/clasify → lsp) reflects what tools actually return; flag guidance that asks for a field the tool does not emit.
- **Reasoning/goal fields.** Optional `goal`/`reasoning` must not be required to get correct results, and output should not echo them back as filler.
- **Confidence vocabulary.** `kind`, `confidence`, coverage, and `lowSignal` states are consistent across tools and tell the agent when to verify.
- **Batching.** Instructions tell agents when to batch independent queries (1–5) and when a dependent query must wait.

## Method

Replay a realistic task end to end with `$OCTO` (and MCP) as an agent would, starting from the instructions alone. Log each moment you had to guess the next call — each guess is a finding. For routing claims across models, use `octocode-eval-benchmark` rather than one replay.

Next: load `references/config-docs-audit.md`, then `references/fix-and-verify.md` for any fix.
