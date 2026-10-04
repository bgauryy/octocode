ea954809ce is dated 2026-09-26. That is after both PRs merged in August 2026, so both are in it.

- **Feature PR:** #18091, "scrape: support scraping targets via Unix Domain Sockets", by IngmarStein. It merged 2026-08-06 as `c5fa89db`. It permits `unix:` addresses in `config.CheckTargetAddress`, makes `scrape.Target.URL()` build `unix`-scheme URLs with the socket path in a query parameter, and adds a custom RoundTripper and dialer interception.
- **Issue closed:** #12024. The PR body says "Fixes #12024" and the release note cites it. I did not open the issue itself.
- **Fix PR:** #19399, "scrape: use a dedicated HTTP client per unix socket target", by roidelapluie. It merged 2026-08-14 as `05f9eb8b`.
- **The mix-up:** the feature passed the socket path to a shared HTTP client's `DialContext` through the request context. The transport keys idle pooled connections only on scheme and `host:port`. Two targets with the same `__address__` but different `__scrape_unix_socket__` paths could therefore reuse each other's pooled connections and scrape the wrong endpoint. A plain TCP target sharing that address could hit the same problem. The fix gives each socket path its own scrape client, cached per scrape pool and rebuilt on reload. The PR body also says its first commit adds a failing test for the bug.

I read the PR titles, bodies and merge dates, not the code diffs.