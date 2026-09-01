# Verification - GitHub history

Verify both halves of the history contract:

| Tool | Operations | Identity |
|---|---|---|
| `ghSearchHistory` | `pullRequests`, `issues`, `commits` | repository plus search filters |
| `ghGetHistoryItem` | `pullRequest`, `issue`, `commit`, `compare` | `number`, `ref`, or `base` + `head` |

Run list searches for all three plural operations, then fetch one result through
the matching singular operation. For `compare`, verify that both `base` and
`head` are required. Confirm the real MCP catalog contains both descriptors in
canonical order and neither descriptor exposes `outputSchema`.
