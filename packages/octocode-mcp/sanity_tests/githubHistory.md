# Verification - GitHub history

Verify both halves of the history contract:

| Tool | Operations | Identity |
|---|---|---|
| `ghSearchHistory` | `pullRequest`, `issue`, `commit` | repository plus search filters |
| `ghGetHistoryItem` | `pullRequest`, `issue`, `commit`, `compare` | `number`, `ref`, or `base` + `head` |

Run list searches for all three singular operations, then fetch one result through
the matching singular operation. For `compare`, verify that both `base` and
`head` are required. Confirm the real MCP catalog contains both descriptors in
canonical order, preserve their input schemas and annotations, and do not expose
an `outputSchema` on either descriptor.
