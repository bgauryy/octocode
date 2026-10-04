**Answer:** undici 6.21.0 declares Node.js `>=18.17`. It was published from commit `61ec3531a64ffeec953a990c11735ff09455de4e`. A `Client` defaults to 300 s for response headers, 300 s for response bodies and 4 s for keep-alive.

- **Node.js support:** the npm metadata for 6.21.0 gives `engines: ">=18.17"`. I did not open `package.json` itself.
- **Source commit:** the git tag `v6.21.0` resolves to `61ec3531a64ffeec953a990c11735ff09455de4e`, "Bumped v6.21.0" by Matteo Collina on 2024-11-13. It changes only `package.json`. That is the same SHA the npm release metadata points to, though the registry tool labelled that mapping "unverified". I did not check whether the registry's `gitHead` field says the same thing.
- **Default timeouts**, from `lib/dispatcher/client.js` at that commit:
  - **Headers:** `headersTimeout` defaults to `300e3` ms (300 s) (`:239`).
  - **Body:** `bodyTimeout` defaults to `300e3` ms (300 s) (`:238`).
  - **Keep-alive:** `keepAliveTimeout` defaults to `4e3` ms (4 s) (`:229`). The related `keepAliveMaxTimeout` is 600 s (`:230`) and `keepAliveTimeoutThreshold` is 2 s (`:231`).

There is one nuance on keep-alive. The 4 s is only the fallback. If the server sends a `Keep-Alive: timeout=` header, that value is used instead, capped by `keepAliveMaxTimeout` and reduced by the threshold. I did not read that code.