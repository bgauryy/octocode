# Sanity check — `ghSearch`

Manual runtime checks for the unified GitHub discovery tool.

## Contract

- [ ] The compact schema exposes `operation:"code"`, `"repositories"`, and `"tree"` with operation-scoped fields.
- [ ] Removed tool names and legacy aliases are rejected with a short canonical-field hint.
- [ ] `pageSize` controls results returned on one page; `page` selects the page.
- [ ] No `limit` is advertised as a client total cap when GitHub provides no such distinct contract.

## Workflow

- [ ] Run one representative query for every operation and verify paths, repository identities, and counts against GitHub.
- [ ] Repeat a query with a small `pageSize`; follow `next` and verify that it preserves `operation`, `pageSize`, and filters while incrementing `page`.
- [ ] Walk response-character pagination when present and verify that no serialized content is silently dropped.
- [ ] Repeat the same request and verify a cached response is marked `cache:1` without extra payload.

## Example

```json
{"queries":[{"operation":"code","searchText":"defineConfig","owner":"vitejs","repo":"vite","pageSize":10,"page":1}]}
```
