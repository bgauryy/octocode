**Answer:** undici 6.21.0 declares Node.js `>=18.17`. The `v6.21.0` tag points to commit `61ec3531a64ffeec953a990c11735ff09455de4e`. A default `Client` uses 300 s for response headers, 300 s for response body, and 4 s for keep-alive.

- **Node versions:** `package.json` at tag `v6.21.0` has `"version": "6.21.0"` (line 3) and `"engines": { "node": ">=18.17" }` (line 134).
- **Source commit:** `gh api repos/nodejs/undici/git/ref/tags/v6.21.0` returns a commit-type ref with SHA `61ec3531a64ffeec953a990c11735ff09455de4e`.
- **Defaults** (`lib/dispatcher/client.js` at `v6.21.0`):
  - **Headers timeout:** `headersTimeout` defaults to `300e3` ms (300 s), set at line 239.
  - **Body timeout:** `bodyTimeout` defaults to `300e3` ms (300 s), set at line 238.
  - **Keep-alive timeout:** `keepAliveTimeout` defaults to `4e3` ms (4 s), set at line 229.
  - **Related keep-alive settings:** the same file sets `keepAliveMaxTimeout` to `600e3` ms (line 230) and `keepAliveTimeoutThreshold` to `2e3` ms (line 231). The max is the cap on server keep-alive hints. The threshold is subtracted from those hints.
  - **Docs:** `docs/docs/api/Client.md` lines 22–26 state the same values.

**Uncertainty:** I got the commit from the git tag, not from the npm registry's `gitHead` field. The registry lookup failed in this sandbox. The tag almost certainly matches what was published, but I did not confirm it against the published tarball.