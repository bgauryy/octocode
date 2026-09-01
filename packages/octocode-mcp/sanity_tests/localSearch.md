# Sanity check — `localSearch`

Manual runtime checks for unified local text, structural, file, and tree discovery.

## Contract

- [ ] The compact schema exposes strict `text`, `structural`, `files`, and `tree` variants with operation-scoped fields.
- [ ] Text and structural operations reject `limit`; they paginate with `pageSize` and `page`.
- [ ] Files and tree operations accept both `limit` and `pageSize`: `limit` caps the total candidate set before pagination, while `pageSize` caps one returned page.
- [ ] Removed tool names and legacy aliases are rejected with a short canonical-field hint.

## Workflow

- [ ] Run one representative query for every operation and verify paths, matches, totals, and operation-specific output.
- [ ] For files or tree, run `{limit:3,pageSize:2,page:1}`, follow `next`, and verify pages contain two then one result.
- [ ] Verify `next` preserves `limit`, `pageSize`, filters, and operation while incrementing `page`.
- [ ] Repeat the same request and verify a cached response is marked `cache:1` without extra payload.

## Example

```json
{"queries":[{"operation":"files","path":"/ABS/repo","names":["*.ts"],"limit":20,"pageSize":5,"page":1}]}
```
