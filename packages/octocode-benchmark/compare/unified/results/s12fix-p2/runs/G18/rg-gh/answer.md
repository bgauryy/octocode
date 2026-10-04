**Answer:** undici 6.21.0 declares Node.js `>=18.17`. The tag `v6.21.0` points at commit `61ec3531a64ffeec953a990c11735ff09455de4e`. A `Client` defaults to 300 s for response headers, 300 s for response bodies and 4 s for keep-alive.

- **Node versions:** `package.json` at tag v6.21.0 has `"engines": { "node": ">=18.17" }` (line 133–135).
- **Source commit:** `gh api repos/nodejs/undici/git/ref/tags/v6.21.0` returns commit `61ec3531a64ffeec953a990c11735ff09455de4e`.
- **Default timeouts**, in `lib/dispatcher/client.js` at that tag:
  - **Headers:** `headersTimeout` defaults to `300e3` ms (300 s), at line 239.
  - **Body:** `bodyTimeout` defaults to `300e3` ms (300 s), at line 238.
  - **Keep-alive:** `keepAliveTimeout` defaults to `4e3` ms (4 s), at line 229. A server keep-alive hint can override it.
  - **Related keep-alive limits:** `keepAliveMaxTimeout` is `600e3` ms (line 230) and `keepAliveTimeoutThreshold` is `2e3` ms (line 231).
  - `docs/docs/api/Client.md` documents the same defaults.

**Uncertainty:** I could not query the npm registry because `curl` was blocked, so I did not see the `gitHead` of the published tarball. The commit above is the one the git tag points to. It is the likely publish source, but I did not confirm that the tarball was built from it.