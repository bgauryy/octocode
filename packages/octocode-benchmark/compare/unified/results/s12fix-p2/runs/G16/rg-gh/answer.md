**Answer:** PR #18091 added Unix domain socket scraping and fixed issue #12024. PR #19399 later fixed the connection mix-up. Both were merged before ea954809ce, which is dated 2026-09-26.

- **Feature PR:** #18091, "scrape: support scraping targets via Unix Domain Sockets". It was merged on 2026-08-06 as commit c5fa89db085c9b3855d7916f2ade047066a7a318.
- **Issue closed:** #12024. The PR body says "Fixes Fixes #12024", and its release note also cites #12024.
- **Fix PR:** #19399, "scrape: use a dedicated HTTP client per unix socket target". It was merged on 2026-08-14 as commit 05f9eb8b3b8e10b48c8f4153b0714dbe9bc9a630.
- **The mix-up:** The original feature passed the socket path to a shared HTTP client's `DialContext` through the request context. The transport keys idle pooled connections only on scheme and `host:port`. So two targets with the same `__address__` but different `__scrape_unix_socket__` paths could reuse each other's pooled connections and scrape the wrong endpoint. A plain TCP target sharing that address was affected the same way.
- **The fix:** Each unix socket path gets its own scrape client. The clients are cached per scrape pool and rebuilt on reload.

**Uncertainty:** I read the PR titles, bodies, merge dates and the commit date through the GitHub API only. I did not inspect the source at ea954809ce, so I did not confirm that the fix is still present in that tree.