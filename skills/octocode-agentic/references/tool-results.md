Load when shaping what a tool returns: result size, references to stored output, or pagination.

# Tool results and pagination

## Tool output design

| Poor response | Better response |
|---|---|
| Entire repository tree | Relevant paths, counts and matching modules |
| Full build log | Status, errors, first actionable locations, artifact reference |
| Entire database rows | Requested fields, filtered records, cursor |
| Unbounded search hits | Ranked top results and `hasMore` |
| Large file | Symbol metadata and relevant ranges |
| Full test output | Pass/fail counts, failures, log reference |
| Raw exception flood | Typed failure, cause, retryability |
| Entire web page | Title, relevant excerpt, source reference |
| Full subagent transcript | Evidence-backed summary and outstanding risks |

Results have a bounded default, with action-relevant fields first, then source locations, completeness and recovery information, and visible security and capability diagnostics. Include counts, revisions and diagnostic details when they support the task; opaque IDs or raw payloads only for evidence or continuation; `response_format: concise | detailed` only for a distinct evidence need.

## Reference-first responses

Tool results should not automatically become model context. Keep the full result in a store and give the model a compact description plus a way to read more.

```text
Tool runs the analysis (repository scan, large build, database query)
  → Artifact store keeps the complete output under a stable reference
  → Agent receives status · summary · relevant findings · resource ID · next actions
  → Agent reads a range, symbol, page or artifact only if needed
```

A reference is server-minted and opaque, resolves for as long as the workflow needs it, is scoped to the caller's identity and tenant, and is reauthorized on every read. A file path alone does not prove the file exists or is current. Default: return a summary and a reference; inline raw data only when it is small and needed.

## Pagination contract

Design pagination into search, listing, scanning and retrieval tools from the start.

1. Bounded default page sizes with server-enforced limits.
2. Opaque cursors over offsets for frequently changing data.
3. Filtering, projection and stable sort order.
4. Source revision or snapshot identity when consistency matters.
5. Direct requests for a specific range, file, record or symbol.
6. Return complete pages with executable continuations; every `hasMore` needs a reachable continuation. Never silently truncate evidence or claim completeness from a partial page; a terminal limit or denied scope is explicit.
7. Tiny results may return whole; pagination is not mandatory for them.
8. Continuations and warnings name what remains: a lead carrying more entries is not named after an enrichment; a warning never implies completeness.
9. Required follow-ups (next page, restart after a stale snapshot) are required leads, never optional items. Server instructions state how a complete result looks, for example no `hasMore` and no continuation.
10. Partial results are visible, for example `isPartial` with reasons; instructions say what the agent may conclude from them.

MCP list operations use opaque cursors: clients follow `nextCursor` and never parse it. Consumer rules are in [the evidence loop](evidence-loop.md#pagination).
