# Sanity check — `ghSearchRepo`, `ghSearchCode`, `ghStructure`

Manual runtime checks for the three GitHub discovery tools.

## Contract

- [ ] Each tool exposes one query shape with no `operation` field; sending `operation` is rejected as an unknown field.
- [ ] `ghSearchCode` requires `owner`; `ghStructure` requires `owner` and `repo`; `ghSearchRepo` needs keywords, topics, owner, or a filter.
- [ ] Removed tool names (`ghSearch`) and legacy aliases are rejected with a short canonical-field hint.
- [ ] `pageSize` controls results returned on one page (1–100 search, 1–200 structure); `page` selects the page.
- [ ] No `limit` is advertised as a client total cap when GitHub provides no such distinct contract.

## Workflow

- [ ] Run one representative query per tool and verify paths, repository identities, and counts against GitHub.
- [ ] Repeat a query with a small `pageSize`; follow `next` and verify that it names the same tool and preserves `pageSize` and filters while incrementing `page`.
- [ ] Follow `ghSearchCode` `next.readTopMatch` (ghGetFileContent) and a zero-hit `next.viewStructure` (ghStructure).
- [ ] Walk response-character pagination when present and verify that no serialized content is silently dropped.
- [ ] Repeat the same request and verify a cached response is marked `cache:1` without extra payload.

## Examples

```json
{"queries":[{"keywords":["defineConfig"],"owner":"vitejs","repo":"vite","pageSize":10}]}
```

```json
{"queries":[{"owner":"vitejs","repo":"vite","path":"packages","maxDepth":2}]}
```
