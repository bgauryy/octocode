**Short answer:** undici 6.21.0 declares Node.js `>=18.17`. The `v6.21.0` tag points at commit `61ec3531a64ffeec953a990c11735ff09455de4e`. A `Client` defaults to 300 s for response headers, 300 s for response bodies and 4 s for keep-alive.

- **Node.js support:** `package.json` at tag `v6.21.0` has `"engines": { "node": ">=18.17" }` (`package.json:133-135`). The same file has `"version": "6.21.0"` at line 3.

- **Source commit:** the tag `v6.21.0` in nodejs/undici resolves to commit `61ec3531a64ffeec953a990c11735ff09455de4e`. I got this from the git ref API. I could not read the npm registry's `gitHead` field because curl was blocked. So this is the release tag's commit, not a value confirmed from the published package metadata.

- **Default timeouts**, from `lib/dispatcher/client.js` at `v6.21.0`:
  - **Response headers:** `headersTimeout` defaults to `300e3` ms (300 s) (`client.js:239`).
  - **Response body:** `bodyTimeout` defaults to `300e3` ms (300 s) (`client.js:238`).
  - **Keep-alive:** `keepAliveTimeout` defaults to `4e3` ms (4 s) (`client.js:229`). Server keep-alive hints can override it. Those hints are capped by `keepAliveMaxTimeout`, which defaults to `600e3` ms (10 min) (`client.js:230`). A `keepAliveTimeoutThreshold` of `2e3` ms (2 s) is subtracted from those hints (`client.js:231`).
  - These match `docs/docs/api/Client.md:22-26`.