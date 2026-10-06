# Runtime check — `localSearch`

Manual runtime checks for lexical local text and regex discovery.

## Contract

- [ ] The compact schema requires `path` and `matchString`; it has no `operation` field.
- [ ] Lexical queries paginate with `pageSize` and `page` (and per-file matches with `matchPage`).
- [ ] Result views include `paginated`, `discovery`, `content`, and `files`; structural, file, tree, and symbol queries use `astSearch`; topology queries use `astTopology`.
- [ ] Removed tool names and legacy aliases are rejected with a short canonical-field hint.

## Workflow

- [ ] Run a representative lexical query and verify paths, matches, totals, and anchors.
- [ ] Run `{pageSize:2,page:1}`, follow `next`, and verify the continuation preserves `matchString`, filters, and page state.
- [ ] Verify per-file match pagination when a file has more matches than `matchPageSize`.
- [ ] Without `pageSize`/`matchPageSize`, a result within ~24 KB is one page, and a larger one is walked by `next.nextPage` alone in ~24 KB pages that show every row once.
- [ ] Repeat the same request and verify a cached response is marked `cache:1` without extra payload.

## Example

```json
{"queries":[{"path":"/ABS/repo","matchString":"needle","regex":"literal","resultView":"files","pageSize":5,"page":1}]}
```
