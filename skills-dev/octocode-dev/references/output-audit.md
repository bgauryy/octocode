# Output audit

Load when evaluating what a tool returns through CLI and MCP. Why: output is the agent's only evidence; loss, noise, and rigid shapes cost every downstream call.

The normative rules are in `docs/TOOL_QUALITY.md` (this skill) (Lossless reachable pagination, Evidence and output integrity) and `<repo>/docs/TOOL_DATA_CONTRACT.md`. This lane executes them.

## Completeness — nothing silently lost

- Every partial surface (result lists, match pages, nested graph data, diagnostics, patches, comments, char windows) has an executable `next.*` continuation or a typed terminal-limit diagnostic.
- Execute continuations until termination; prove the union equals the fixture: no gaps, no repeats, stable identities, filters/view/ref preserved.
- No hard truncation without a marker + continuation: grep the tool module for `truncate`, `take(`, `.min(`, `[..`, byte/char caps, and clip constants; each must surface in output metadata.
- Windows glued together need a gap marker; clipped lines/context need a flag.

## Redundancy — nothing said twice

- Values repeated per row that could hoist to `shared`/`base` (CLI compact already hoists — check MCP structured output too).
- Echoed input fields the agent already has; debug-only fields leaking into default output; both text and structured content carrying divergent data.
- Evidence repeated across `meta.evidence`, diagnostics, and hints.
- Measure chars per useful fact before/after with a fixed query.

## Rigidity and bad parts

- Fixed-size windows, hard-coded counts, or thresholds that should derive from input or config.
- Regex used to parse structured data (JSON, code, diffs, URLs) where a parser/AST exists; regex over user input without the isolated worker.
- String-built output that a typed struct should produce; ad-hoc formatting per tool instead of shared `response/` helpers.
- Enum/status strings invented per tool instead of the shared vocabulary.

## Integrity

- `none` view equals source bytes after expected redaction; transformed views (`standard`, `symbols`, minify) keep matched text and correct line anchors.
- Mixed success/error batches keep `index` alignment; an outer success never hides a failed row.
- Secrets redacted in every surface, including clipped values and match strings.

## Record

Per finding: surface (CLI/MCP/both), reproducing call, observed vs expected payload excerpt (short), file:line of the producer.

Next: load `references/workflow-audit.md` to judge how agents chain these results.
