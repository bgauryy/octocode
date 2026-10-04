**Answer:** PR #18091 added Unix domain socket scraping and fixed issue #12024. PR #19399 later fixed the connection mix-up.

- **Feature PR:** #18091, "scrape: support scraping targets via Unix Domain Sockets". It was merged 2026-08-06 as `c5fa89db085c9b3855d7916f2ade047066a7a318`. The description says "Fixes Fixes #12024", and the release note is "[FEATURE] Scraping: Add support for scraping targets via Unix Domain Sockets. #12024". I didn't open issue #12024 itself, so I can't confirm it was closed. I'm relying on the PR text.
- **Fix PR:** #19399, "scrape: use a dedicated HTTP client per unix socket target". It was merged 2026-08-14 as `05f9eb8b3b8e10b48c8f4153b0714dbe9bc9a630`.
- **The mix-up:** The feature passed the socket path to a shared HTTP client's `DialContext` through the request context. The transport keys idle pooled connections only on scheme and `host:port`. So two targets with the same `__address__` but different `__scrape_unix_socket__` paths could reuse each other's pooled connections and scrape the wrong endpoint. A plain TCP target sharing that address was affected the same way. The fix gives each unix socket path its own scrape client. These clients are cached per scrape pool and rebuilt on reload.

**Uncertainty:**
- I found these PRs through search and the GitHub API, not by reading the code at `ea954809ce`.
- That commit is dated 2026-09-26, after both merges, so both PRs should be in it. I didn't check that ancestry directly.
- I haven't checked whether other PRs touched this feature in between.