Load when evaluating what a tool returns via CLI and MCP, or whether an agent can chain it. Rules: `docs/TOOL_QUALITY.md` and `<repo>/docs/TOOL_DATA_CONTRACT.md`; this lane executes them.

# Output and workflow audit

## Completeness: nothing silently lost
- A follow-up the result is complete without belongs in typed `related[]`, never `next`. Required continuation chains terminate without cycling through optional reads.
- Paginate before size reduction. Search the producer for `truncate`, `take(`, `.min(`, `[..`, byte/char caps and clip constants; complete selected values must reach the shared pager. A recovery read does not justify clipping a selected value.
- Disjoint windows need a gap marker; resource or upstream limits disclose the unreachable scope; oversized indivisible values stay whole and disclosed.

## Redundancy: nothing said twice
- Metadata or summaries repeating returned evidence (keep independently readable rows; add no second hoisting format), echoed inputs, duplicated navigation, diverging text and structured content, evidence repeated across `meta.evidence`, diagnostics and hints. Measure chars per useful fact before and after with a fixed query.

## Rigidity and integrity
- Fixed windows or hard-coded counts that should derive from input or config; regex parsing structured data where a parser or AST exists; regex over user input without the isolated worker.
- String-built output a typed struct should produce; per-tool formatting instead of shared `response/` helpers; per-tool enum or status strings instead of the shared vocabulary.
- Secrets redacted on every surface, clipped values and match strings included.

## Workflow: the next call is obvious
- Check hits, empty, partial and error results. Remaining requested evidence needs an executable `next.*`; optional exploration uses `related[]`; correction tips use prose `hints[]`. No navigation for a complete result, no silent repair of invalid input.
- A typed call names its tool and complete input from observed path, line, SHA or symbol values. Validate every emitted input against the receiving tool. Flag repeated tips and calls to unavailable tools.
- Producer fields feed consumer inputs without reshaping: search to fetch (`path` + line/anchor), ghSearchCode/ghStructure to ghGetFileContent (owner/repo/ref), history to item (number/SHA), astSearch topology to lspSearch. Check against "Connections between tools" in `<repo>/docs/TOOL_DATA_CONTRACT.md`.
- The instructions' locate cascade (anchor, search, fetch, rerank/clasify, lsp) matches what tools return; flag guidance asking for a field the tool does not emit.
- Classification uses the declared `context`, sent with each question. A source row's typed saved read retrieves exactly the judged selection at its snapshot or commit; execute it. No search or fetch result offers a `clasify` handoff or automatic reread ([guide](../../../docs/OCTOCODE_CLASIFY.md#what-every-call-does)).
- `kind`, `confidence`, coverage and `lowSignal` are consistent across tools and say when to verify.
- Instructions say when to batch independent queries (1-5) and when a dependent one waits.

Method: replay a realistic task end to end with `$OCTO` (and MCP) from the instructions alone; each guessed next call is a finding. Routing claims across models need `octocode-agentic` measurement.

Record per finding: surface (CLI/MCP/both), reproducing call, short observed vs expected excerpt, producer `file:line`.

Next: `references/contract-audit.md` Config and docs, then `references/fix-and-verify.md` for any fix.
