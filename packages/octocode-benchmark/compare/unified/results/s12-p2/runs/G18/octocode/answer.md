**Node.js support:** undici@6.21.0 declares `engines: ">=18.17"` (Node.js 18.17 and later). This comes from the npm registry metadata. I did not open `package.json` itself.

**Source commit:** `61ec3531a64ffeec953a990c11735ff09455de4e`. It is the commit that the `v6.21.0` tag resolves to in nodejs/undici. The message is "Bumped v6.21.0", authored by Matteo Collina on 2024-11-13, and it changes only `package.json`. The npm release metadata points to the same SHA, but marks that link as "unverified". My confirmation is the tag match.

**`Client` defaults** (`lib/dispatcher/client.js` at that commit):
- **Response headers timeout:** 300 s. `headersTimeout` defaults to `300e3` ms (line 239).
- **Response body timeout:** 300 s. `bodyTimeout` defaults to `300e3` ms (line 238).
- **Keep-alive timeout:** 4 s. `keepAliveTimeout` defaults to `4e3` ms (line 229). The related defaults are `keepAliveMaxTimeout` of 600 s (line 230) and `keepAliveTimeoutThreshold` of 2 s (line 231).

A server-supplied `Keep-Alive` header can override the 4 s keep-alive default. The server's value is capped at `keepAliveMaxTimeout`. I did not read that logic in this release.