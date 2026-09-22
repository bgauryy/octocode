# Runtime check — `localSearch`

Manual runtime checks for lexical local text and regex discovery.

## Contract

- [ ] The compact schema requires `path` and `searchText`; it has no `operation` field.
- [ ] Lexical queries paginate with `pageSize` and `page` (and per-file matches with `matchPage`).
- [ ] Result views include `paginated`, `discovery`, `content`, and `files`; structural, file, tree, and symbol queries use `astSearch`; topology queries use `astTopology`.
- [ ] Removed tool names and legacy aliases are rejected with a short canonical-field hint.

## Workflow

- [ ] Run a representative lexical query and verify paths, matches, totals, and anchors.
- [ ] Run `{pageSize:2,page:1}`, follow `next`, and verify the continuation preserves `searchText`, filters, and page state.
- [ ] Verify per-file match pagination when a file has more matches than `maxMatchesPerFile`.
- [ ] Repeat the same request and verify a cached response is marked `cache:1` without extra payload.

## Example

```json
{"queries":[{"path":"/ABS/repo","searchText":"needle","regex":"literal","resultView":"files","pageSize":5,"page":1}]}
```
