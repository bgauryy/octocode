# Output

Lead with the answer to the structural question, then the rows that support it.

| Part | Include |
|---|---|
| Scope | The `path` built, the snapshot `id` and whether `stale` reported it fresh |
| Rows | Each cited node or edge with `path:line`, the operation that returned it and its confidence level |
| Counts | Whether the count is complete or a floor (`hasMore`, unread pages) |
| Proof | The `lspSearch` references and source spans read; label a row that was not checked as a lead |
| Gaps | `unresolved` reasons, unmodeled dynamic behavior and scopes not built |

A saved result goes under `<workspace>/.octocode/octocode-graph/` (scratch under `<workspace>/.octocode/octocode-graph/tmp/`); a path the user names takes precedence.
