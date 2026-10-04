undici 6.21.0 declares Node.js `>=18.17`. It was published from commit `61ec3531a64ffeec953a990c11735ff09455de4e`. A `Client` defaults to 300 s for response headers, 300 s for response bodies and 4 s for keep-alive.

- **Node.js support:** `package.json` has `"engines": { "node": ">=18.17" }` at line 134 at that commit. The npm registry metadata shows the same value.
- **Source commit:** the git tag `v6.21.0` resolves to that SHA. It is the commit "Bumped v6.21.0" by Matteo Collina, dated 2024-11-13, and it changes only `package.json`. Its parent is `11e31a4f…`. The npm metadata gives the same SHA as the release source, but marks that mapping "unverified". The tag match is my own check.
- **Default timeouts**, all in `lib/dispatcher/client.js` at that commit:
  - **Response headers (`headersTimeout`):** 300e3 ms (300 s), line 239.
  - **Response body (`bodyTimeout`):** 300e3 ms (300 s), line 238.
  - **Keep-alive (`keepAliveTimeout`):** 4e3 ms (4 s), line 229.
  - **Related keep-alive limits:** `keepAliveMaxTimeout` defaults to 600e3 ms (600 s), line 230. `keepAliveTimeoutThreshold` defaults to 2e3 ms (2 s), line 231. I added these two; you didn't ask for them.

I didn't check whether the published npm tarball differs from the git tag.