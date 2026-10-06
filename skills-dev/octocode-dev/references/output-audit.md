# Output and workflow audit

Load when evaluating what a tool returns through CLI and MCP, or whether an agent will chain it into the next call. The normative rules are in `docs/TOOL_QUALITY.md` (Lossless reachable pagination, Evidence and output integrity) and `<repo>/docs/TOOL_DATA_CONTRACT.md`. This lane executes them.

## Completeness — nothing silently lost
- A follow-up the result is complete without belongs in `hints`, never in `next`.
- No hard truncation without a marker and a continuation. Grep the tool module for `truncate`, `take(`, `.min(`, `[..`, byte/char caps, and clip constants; each must surface in output metadata.
- Windows glued together need a gap marker; clipped lines or context need a flag.

## Redundancy — nothing said twice
- Values repeated per row that could hoist to `shared`/`root` (CLI compact already hoists; check MCP structured output too).
- Echoed input fields, debug-only fields in default output, and text and structured content that carry divergent data.
- Evidence repeated across `meta.evidence`, diagnostics, and hints. Measure chars per useful fact before and after with a fixed query.

## Rigidity and integrity
- Fixed-size windows, hard-coded counts, or thresholds that should derive from input or config.
- Regex that parses structured data (JSON, code, diffs, URLs) where a parser or AST exists; regex over user input without the isolated worker.
- String-built output that a typed struct should produce; per-tool formatting instead of shared `response/` helpers; per-tool enum or status strings instead of the shared vocabulary.
- Secrets are redacted in every surface, including clipped values and match strings.

## Workflow — the next call is obvious
- Every result state (hits, zero hits, too many hits, error, partial) returns an executable `next.*` page, a `hints.*` lead, or a `hints.text` tip that names the correction. Zero-result and invalid-input paths matter most.
- A hint names the tool and fields to call next, with values from this result (path, line, SHA, symbol). Flag generic advice, hints repeated on every row, and hints to disabled tools.
- Producer fields feed consumer inputs without reshaping: search → fetch (`path` + line/anchor), ghSearchCode/ghStructure → ghGetFileContent (owner/repo/ref), history → item (number/SHA), astSearch topology → lspSearch confirmation. Check against `<repo>/docs/TOOL_DATA_CONTRACT.md` "Connections between tools".
- The instructions' locate cascade (anchor → search → fetch → rerank/clasify → lsp) matches what tools return. Flag guidance that asks for a field the tool does not emit.
- Classification matrices follow the same optional-brief rule. Successful output need not echo the brief. A `hints.clasify` handoff needs `mainGoal`.
- `kind`, `confidence`, coverage, and `lowSignal` states are consistent across tools and tell the agent when to verify.
- Instructions say when to batch independent queries (1–5) and when a dependent query must wait.

Method: replay a realistic task end to end with `$OCTO` (and MCP), starting from the instructions alone. Each moment you must guess the next call is a finding. For routing claims across models, use `octocode-eval-benchmark`, not one replay.

Record per finding: surface (CLI/MCP/both), reproducing call, short observed vs expected excerpt, producer `file:line`.

Next: load `references/contract-audit.md` § Config and docs, then `references/fix-and-verify.md` for any fix.
