**Answer:** PR #18091 added Unix domain socket scraping and closed issue #12024. PR #19399 later fixed the connection mix-up.

- **Feature PR:** #18091, "scrape: support scraping targets via Unix Domain Sockets". It was merged on 2026-08-06 as `c5fa89db08`.
  - Its description says "Fixes #12024", the issue "Support Unix socket for metrics address".
  - Its release note reads "[FEATURE] Scraping: Add support for scraping targets via Unix Domain Sockets. #12024".
- **Fix PR:** #19399, "scrape: use a dedicated HTTP client per unix socket target". It was merged on 2026-08-14 as `05f9eb8b3b`.
- **The mix-up:**
  - The feature passed the socket path to a shared HTTP client's `DialContext` through the request context.
  - The transport keys idle pooled connections only on scheme and `host:port`.
  - So two targets with the same `__address__` but different `__scrape_unix_socket__` paths could reuse each other's pooled connections and scrape the wrong endpoint.
  - The same could happen to a plain TCP target that shared that address.
  - The fix gives each unix socket path its own scrape client, cached per scrape pool and rebuilt on reload.

**Commit check:** The compare API reports `ea954809ce` as "ahead" of both merge commits, so both PRs are in that commit.

**Not checked:** I didn't read the source at that commit. The fix details come from the PR #19399 description, not from the diff.