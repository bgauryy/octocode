# Sanity check — `lspFindReferences`

> All references/usages of a symbol.
>
> **Manual runtime checklist** — run each step against the live MCP tool (or `benchmark/github/scripts/call-tool.mjs lspFindReferences '<queries-json>'`) and tick the box.
> Automated schema-contract checks live in `tests/tools/all-tools.pagination-contract.test.ts`; cursor-uniformity in `tests/tools/all-tools.pagination.test.ts`.

## 1. Scheme
- [ ] Tool is registered and accepts the bulk envelope: `queries[]`, `responseCharOffset`, `responseCharLength`, `format`.
- [ ] Pagination knob(s) accepted: `referencesPerPage`, `page`.
- [ ] `verbosity` accepted: `basic` (default) / `compact` / `concise`.
- [ ] A minimal valid query parses (see Example).

## 2. Pagination (lossless — nothing silently dropped)
- [ ] Run a query that returns a large result with a small `responseCharLength` (e.g. `500`): the response is **bounded** AND `responsePagination.hasMore = true` with a `Page N/M … Next: responseCharOffset=…` hint.
- [ ] Walk the cursor (`responseCharOffset`) to the end — **every item is reachable**, nothing missing vs. the reported total.
- [ ] No `… [truncated]` / `… [clipped]` marker appears anywhere in the output.
- [ ] Per-query knob (`referencesPerPage` / `page`) returns the next page on increment.
- [ ] Row-paginates by `referencesPerPage`; intentionally skipped from char-pagination today.

## 3. Quality
- [ ] Results are correct for a known query (spot-check against the real source).
- [ ] Counts / `hasMore` reflect the **true** total (no silent drop).
- [ ] Identifiers (paths, owners, line numbers, SHAs) are accurate and not mangled.

## 4. Token effectiveness
- [ ] Output is lean structured YAML — `base` relativizes paths and `shared` hoists constants shared across rows (no per-call `format` knob).
- [ ] `verbosity:"concise"` yields a **strictly smaller** payload than `basic` for the same query.
- [ ] The size knob (`referencesPerPage` / `page`) lets you fetch exactly what you need — the tool never over-returns.

## Known gaps
- ⚠️ Over-budget `locations` are **tail-dropped** (clampContentBudget) rather than char-paged; narrow `contextLines`/`referencesPerPage` to fit — pagination issue #1 / task #6.

## Example call
```json
{ "queries": [ {"uri":"file:///abs/path.ts","symbolName":"foo","lineHint":10,"referencesPerPage":20,"page":1} ], "responseCharLength": 500 }
```
