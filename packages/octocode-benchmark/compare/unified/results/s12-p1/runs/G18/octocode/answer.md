**Answer:** undici@6.21.0 declares Node.js `>=18.17`. It was published from commit `61ec3531a64ffeec953a990c11735ff09455de4e`. A `Client` defaults to 300 s for response headers, 300 s for response bodies and 4 s for keep-alive.

- **Node.js versions:** `package.json:134` in the `v6.21.0` tag has `"engines": {"node": ">=18.17"}`. The npm registry metadata shows the same `>=18.17`.
- **Source commit:** the tag `v6.21.0` resolves to `61ec3531a64ffeec953a990c11735ff09455de4e`. That commit is "Bumped v6.21.0" by Matteo Collina, dated 2024-11-13, and only changes `package.json`. The same SHA was the release-source hint from the npm metadata, which flagged it as unverified. I only matched it against the git tag, not against anything npm records itself.
- **Default timeouts**, all in `lib/dispatcher/client.js` at that commit:
  - **Response headers (`headersTimeout`):** 300e3 ms, which is 300 s (line 239).
  - **Response body (`bodyTimeout`):** 300e3 ms, which is 300 s (line 238).
  - **Keep-alive (`keepAliveTimeout`):** 4e3 ms, which is 4 s (line 229). This is the default idle keep-alive timeout. A server's `Keep-Alive` header hint can override it, capped at `keepAliveMaxTimeout` (default 600e3 ms, line 230). `keepAliveTimeoutThreshold` defaults to 2e3 ms (line 231).

I did not check the docs or the Pool/Agent classes, since only `Client` was asked about.