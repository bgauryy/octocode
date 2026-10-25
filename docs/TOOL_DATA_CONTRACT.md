# Tool data and handoff contract

CLI and MCP expose one core-owned contract. Native validates inputs, executes tools, sanitizes evidence and checks shaped output. Discovery describes the current contract; source schemas live in octocode-core.

## Requests and result rows

A new request is `{"queries":[query,...],"responseChars":20000}`; the optional responseChars sets a display target. A continuation request is only `{"cursor":"…"}`. There is no bare-query input, field alias conversion or public progress override.

Results are `{"results":[{"index":0,"data":{...}}]}`. `index` is the original query position, preserved through tool and response continuations. When a continuation expands one query into several reads, each result retains that query's index; a fresh related request starts its own indices. A row may carry `status:"empty"`, `"error"` or `"outOfRange"`; successful evidence rows omit status. Failure data carries `errorCode` and `error`, with details and retryability when relevant. Invalid query rows are isolated for every tool, including `clasify`; valid siblings still execute. Invalid envelopes and batches with no valid rows fail as a whole. Inspect each row even if the envelope succeeds.

MCP sets `isError` when every row in the captured batch failed; a page containing only errors can belong to a mixed batch. CLI reports a failure exit code even when valid siblings returned evidence. Rejected input rows appear on the first response; continuations resume admitted work, so their CLI exit code can differ. Follow response continuations and inspect each row before judging the complete result.

### Numbered source content

Numbered source `content` uses `line<TAB>text`. Search match `value` fields use separate source coordinates; do not assume their text has line prefixes. Lines are one-based. Ranges use inclusive `startLine`/`endLine`; columns are one-based UTF-16 positions where documented. Preserve source numbers when citing. Secret redaction precedes range or match selection, so a fragment of a multi-line secret cannot escape scanning.

For reads selected by `matchString`, `matchedLines` identifies actual hit lines on that page. Context lines remain in `content` without becoming hits; `matchedLines:[]` means this page contains no hit lines. The list remains explicit even when every returned line is a hit. Ordinary reads omit it.

### Search result shapes

Text and AST matches group under `files[].path` with `matches[]`. Each match retains its source location and evidence. Local text hits may contain `enclosing:{symbolName,kind,line,endLine}`; every such hit carries the complete declaration. Outline nodes own their path/coordinates and evidence fields rather than requiring inherited hidden defaults.

Structure listings group entries by `dir`. Entries may append a byte count in parentheses; use the name without this display suffix. LSP locations, hierarchy nodes and sites use their declared location objects. An evidence path inside the workspace is workspace-relative, including skipped-path disclosures; an admitted external path remains absolute. The shared native execution boundary applies this rule for every local tool on CLI and MCP.

### Minimal by default

The default response includes evidence, provenance, required coverage facts and executable next steps. Rendered-size counters and cache warmth stay internal. There is no extras switch or hidden field-restoration mode. Size reductions may remove duplicate metadata, never evidence. Choose an explicit view to change the requested representation; compact and symbols views are transformed evidence, not verbatim source.

## Evidence boundaries

A result proves only its stated scope. Preserve `isPartial`, `partialReasons`, warnings, skipped-path disclosures, selection receipts and terminal limits. Search-index gaps, provider caps and unavailable language servers constrain absence claims. A complete page can still belong to an incomplete collection.

Classification counts selected, related, excluded and unknown candidates within the assessed scope. Provider failure or unassessed evidence does not mean irrelevant. Inspect the emitted unfiltered and source-read continuations when the task requires that evidence.

## Executable continuations

```json
{"kind":"continue","tool":"localFetch","input":{"cursor":"<issued cursor>"}}
```

`next.*` continues or recovers saved work. `responsePagination.next` retrieves the rest of the current serialized response; finish those pages before advancing its tool-level continuations. Each emitted call names a tool and a complete input. Execute it unchanged; do not add queries, filters, offsets, responseChars or other fields to a cursor request.

```json
{"kind":"related","tool":"localFetch","input":{"queries":[{"path":"src/index.ts","ranges":[{"startLine":1,"endLine":40}]}]}}
```

`related[]` starts optional new work from public input. It is distinct from remaining evidence. Executable call inputs may use absolute paths so the call retains its target; copy them unchanged. `hints[]` is prose, never a string-encoded command. An empty result may offer broader related searches; each has a different scope that the caller must evaluate.

A request rejected before source capture may offer a fresh related read. It has no captured version to resume. Recovery of evidence that was already captured remains source-bound continuation work.

Cursors are opaque handles bound to tool, configuration and access context. Persistent storage can support a later process; memory mode lasts only in its issuing runtime. Replaying a cursor does not consume it: concurrent resumes retain every published child until expiry or eviction. Expired, foreign or tampered handles fail without calling the source or provider. Changes to a source invalidate a live-source continuation; frozen captures retain their original evidence under their access policy.

A large result returns complete pieces and continuations covering the rest exactly once. A terminal cap must say what is unreachable. A single indivisible value above the response target is disclosed as oversized. Never construct a page number or assume an absent total means zero.

## Paths, shared fields, and anchors

Evidence rows retain their own identity on every response-page fragment, including optional revision pins and qualifiers such as whether a search line was resolved at that revision. There is no shared/root hoisting protocol to reconstruct. Resolve workspace-relative paths against the configured workspace, and use the emitted call when available. A commit SHA, source snapshot or coordinate is provenance; it is not permission to invent private cursor fields.

## Connections between tools

Repository → tree/code → file; history search → item → patch/source; package → release source; clone → local search/AST/LSP; text hit → exact read → semantic anchor. [Workflows](OCTOCODE_WORKFLOWS.md) explain when to follow each path.
