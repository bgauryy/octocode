# Sanity check - `oqlSearch`

> Run OQL research workflows with typed continuations.
>
> **Manual runtime checklist** - run each step against the live MCP tool (or `benchmark/github/scripts/call-tool.mjs oqlSearch '<queries-json>'`) and tick the box.
> Automated schema-contract checks live in `tests/tools/all-tools.pagination-contract.test.ts`; cursor-uniformity in `tests/tools/all-tools.pagination.test.ts`.

## 1. Scheme
- [ ] Tool is registered and accepts either one OQL query or `{ "queries": [...] }`.
- [ ] `where` documents `text`, `regex`, `structural`, `field`, `all`, `any`, and `not`.
- [ ] `target`, `from`, `where`, `fetch`, `params`, `page`, and `itemsPerPage` are visible in the raw schema.
- [ ] A minimal valid query parses (see Example).

## 2. Pagination
- [ ] Local code search with file paging makes the pagination unit explicit (`itemUnit:"files"`).
- [ ] `next.page`, `next.matchPage`, and `next.charRange` are executable as returned.
- [ ] `evidence.answerReady` and `diagnostics` accurately distinguish proof, candidate, partial, and unsupported results.

## 3. Quality
- [ ] `structuredContent.results[].data` contains the OQL envelope for raw MCP/CLI tool calls.
- [ ] `structuredContent` does not duplicate the same envelope under a top-level `oql` mirror.
- [ ] `search --query --json` remains the native OQL envelope, not a CallToolResult.

## 4. Agent Effectiveness
- [ ] The output carries enough `next.*` continuations to move from orient/search to exact reads and LSP proof.
- [ ] Invalid queries return usage-style diagnostics rather than looking like backend failures.
- [ ] Native OQL rows keep stable path, line, source, and continuation anchors.

## Example call
```json
{ "target": "code", "from": { "kind": "local", "path": "." }, "where": { "kind": "text", "value": "registerTool" }, "view": "discovery", "limit": 5 }
```
