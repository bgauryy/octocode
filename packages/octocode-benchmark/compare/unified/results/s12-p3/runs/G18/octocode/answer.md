**Node.js support:** undici@6.21.0 declares `engines: ">=18.17"` in its npm metadata. It was published on 2024-11-13.

**Source commit:** `61ec3531a64ffeec953a990c11735ff09455de4e`. That is the commit the tag `v6.21.0` points to, "Bumped v6.21.0" by Matteo Collina. It changes only `package.json`, and its parent is `11e31a4f…`. The npm metadata gave the same SHA as the release source, but the tool marked that link "unverified". The tag match is my only confirmation of it.

**Client defaults** (`lib/dispatcher/client.js` at that commit):
- **Response headers:** `headersTimeout` defaults to 300e3 ms, which is 5 minutes (`client.js:239`).
- **Response body:** `bodyTimeout` defaults to 300e3 ms, which is 5 minutes (`client.js:238`).
- **Keep-alive:** the default `keepAliveTimeout` is 4e3 ms, which is 4 seconds (`client.js:229`). Three related settings sit alongside it:
  - `keepAliveMaxTimeout` defaults to 600e3 ms, or 10 minutes (`client.js:230`).
  - `keepAliveTimeoutThreshold` defaults to 2e3 ms, or 2 seconds (`client.js:231`).
  - The 4 s default is the starting value only. A server's `Keep-Alive` hint can change it, capped by the max. I didn't read the code that applies that hint.

**Uncertainty:** I didn't check whether `Agent` or the other dispatchers override these defaults. These values are for `Client` only.