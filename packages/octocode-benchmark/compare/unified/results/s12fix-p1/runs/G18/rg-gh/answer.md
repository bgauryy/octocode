**Summary:** undici 6.21.0 declares Node.js `>=18.17`. The `v6.21.0` tag points at commit `61ec3531a64ffeec953a990c11735ff09455de4e`. A `Client` defaults to 300 s (300000 ms) for headers, 300 s for body, and 4 s for keep-alive.

- **Node.js support:** `package.json` at tag `v6.21.0` has `"engines": { "node": ">=18.17" }` (line 133–135).
- **Source commit:** the GitHub ref `refs/tags/v6.21.0` in nodejs/undici resolves to commit `61ec3531a64ffeec953a990c11735ff09455de4e`.
- **Default timeouts**, all in `lib/dispatcher/client.js` at that tag:
  - **Response headers:** `headersTimeout` defaults to `300e3` ms, which is 300 s (line 239).
  - **Response body:** `bodyTimeout` defaults to `300e3` ms, which is 300 s (line 238).
  - **Keep-alive:** `keepAliveTimeout` defaults to `4e3` ms, which is 4 s (line 229). This is only the fallback when the server gives no hint.
  - Related keep-alive settings are `keepAliveMaxTimeout` at `600e3` (line 230) and `keepAliveTimeoutThreshold` at `2e3` (line 231).

**Uncertainty:** I could not read the npm registry (it returned 403), so I could not check the published package's `gitHead`. The commit above comes from the git tag, on the assumption that the tag matches what was published.