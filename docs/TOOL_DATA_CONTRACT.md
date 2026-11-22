# Read tool results

Use `--json` in the CLI to get structured results. MCP returns JSON in its text block by default. To look up a field, run `octocode schema <tool> --view output`.

## Requests

Send independent questions to one tool in `queries`. Each result's `index` is the position of its question, starting at zero. Most tools accept up to five queries per call. `graphBuild` accepts one.

```bash
octocode localFetch '{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}'
```

## Result rows

A successful row contains `data`. An `empty` row describes a search that found nothing in its scope. An `error` row gives a reason and may include a recovery call. A batch can contain both successful and failed rows.

## Shared facts

If a row omits a repository, revision or other repeated field, look in the containing group, the row's `data`, `shared.data` and the original request. For grouped file names, join the directory and file name to get the path.

## Executable continuations

| In the response | How to use it |
|---|---|
| Top-level `next` | Fetch the next page. Send its `input` unchanged to its `tool`. Repeat until the response has no top-level `next`. |
| A row's `next.*` | Open the branch you need, such as a patch, a source read, or a retry. A classifier confirmation can authorize additional paid work. |
| `related` | Start a suggested follow-up query. |
| `warnings`, `isPartial`, `skippedPaths` | Check what the tool could establish and which parts of the requested scope it missed. |

For example, if `next.tool` is `localFetch`, pass the returned cursor as the entire input to `localFetch`. A later page can contain the first result for another query in the same batch. Use the row index to put it in the right place.

## Pages and size

Set `responseChars` on the first call to request a page size. A single indivisible value can exceed it. When a row spans pages, `rowPart` gives the order of its pieces. Append collection items, keep repeated identifiers once and join split text at the returned JSON pointer. Use the output schema's `collectionPaths` when joining results in a script. For source byte windows, use `offset` to put the text in file order.

## Errors and recovery

Fix the field named in an input error. For an expired or changed snapshot, follow `next.restart` and start the result again. Retry a transient failure when the result marks it retryable. [CLI exit codes](OCTOCODE_CLI.md#exit-codes) · [Storage and cursor lifetime](STORAGE_AND_LIMITS.md#cache-storage-and-lifecycle)
