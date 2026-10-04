**Answer:** undici@6.21.0 declares Node.js `>=18.17`. The tag `v6.21.0` points to commit `61ec3531a64ffeec953a990c11735ff09455de4e`. A `Client` defaults to 300 s for response headers, 300 s for response bodies and 4 s for keep-alive.

- **Node versions:** `package.json` at tag v6.21.0 has `"engines": { "node": ">=18.17" }` (lines 133-135) and `"version": "6.21.0"` (line 3).
- **Source commit:** `gh api` resolves tag `v6.21.0` to commit `61ec3531a64ffeec953a990c11735ff09455de4e`. I could not read the npm registry's `gitHead` field because `curl` was blocked. So I have not confirmed that npm published from this exact commit. A release tag normally matches the published commit.
- **Default timeouts**, in `lib/dispatcher/client.js` at v6.21.0:
  - **Headers:** `headersTimeout` defaults to 300e3 ms (300 s), at line 239.
  - **Body:** `bodyTimeout` defaults to 300e3 ms (300 s), at line 238.
  - **Keep-alive:** `keepAliveTimeout` defaults to 4e3 ms (4 s), at line 229. This is a fallback: a server `Keep-Alive` header can override it.
  - **Other keep-alive settings:** `keepAliveMaxTimeout` is 600e3 ms (line 230) and `keepAliveTimeoutThreshold` is 2e3 ms (line 231).