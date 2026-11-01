Load when evaluating what a tool returns through CLI and MCP, or whether an agent will chain it into the next call. The normative rules are in `docs/TOOL_QUALITY.md` (Lossless reachable pagination, Evidence and output integrity) and `<repo>/docs/TOOL_DATA_CONTRACT.md`. This lane executes them.

# Output and workflow audit

## Completeness — nothing silently lost
- A follow-up the requested result is complete without belongs in typed `related[]`, never in `next`. Required continuation chains must terminate without cycling through optional section or source reads.
- Paginate before size reduction. Search the producer for `truncate`, `take(`, `.min(`, `[..`, byte/char caps and clip constants; verify complete selected values reach the shared response pager. A recovery read does not justify clipping an already selected value.
- Selected disjoint windows need a gap marker. Resource or upstream terminal limits must disclose the unreachable scope. Keep oversized indivisible values whole and disclosed.

## Redundancy — nothing said twice
- Duplicate metadata or summaries that repeat already returned evidence. Preserve independently readable rows; do not introduce a second hoisting or restoration format.
- Echoed inputs with no evidentiary role, duplicated navigation, and text and structured content that carry divergent data.
- Evidence repeated across `meta.evidence`, diagnostics, and hints. Measure chars per useful fact before and after with a fixed query.

## Rigidity and integrity
- Fixed-size windows, hard-coded counts, or thresholds that should derive from input or config.
- Regex that parses structured data (JSON, code, diffs, URLs) where a parser or AST exists; regex over user input without the isolated worker.
- String-built output that a typed struct should produce; per-tool formatting instead of shared `response/` helpers; per-tool enum or status strings instead of the shared vocabulary.
- Secrets are redacted in every surface, including clipped values and match strings.

## Workflow — the next call is obvious
- Check hits, empty results, partial results and typed errors. Remaining requested evidence needs an executable `next.*` continuation. Optional exploration uses `related[]`; correction tips use prose `hints[]`. Do not invent navigation for a complete result or silently repair invalid input.
- A typed call names its tool and complete input using observed path, line, SHA or symbol values. Validate every emitted input against the receiving tool. Flag repeated tips and calls to unavailable tools.
- Producer fields feed consumer inputs without reshaping: search → fetch (`path` + line/anchor), ghSearchCode/ghStructure → ghGetFileContent (owner/repo/ref), history → item (number/SHA), astSearch topology → lspSearch confirmation. Check against `<repo>/docs/TOOL_DATA_CONTRACT.md` "Connections between tools".
- The instructions' locate cascade (anchor → search → fetch → rerank/clasify → lsp) matches what tools return. Flag guidance that asks for a field the tool does not emit.
- Classification uses the declared `context`; the provider receives it with each question. A source row's typed saved read retrieves exactly the judged selection at its snapshot or commit; execute it. No search or fetch result offers a `clasify` handoff or an automatic reread of evidence it returned ([classifier and clasify guide](../../../docs/OCTOCODE_CLASIFY.md#shared-mechanism)).
- `kind`, `confidence`, coverage, and `lowSignal` states are consistent across tools and tell the agent when to verify.
- Instructions say when to batch independent queries (1–5) and when a dependent query must wait.

Method: replay a realistic task end to end with `$OCTO` (and MCP), starting from the instructions alone. Each moment you must guess the next call is a finding. For routing claims across models, use `octocode-eval-benchmark`, not one replay.

Record per finding: surface (CLI/MCP/both), reproducing call, short observed vs expected excerpt, producer `file:line`.

Next: load `references/contract-audit.md` § Config and docs, then `references/fix-and-verify.md` for any fix.
