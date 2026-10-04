PR #18091 added Unix domain socket scraping and closed issue #12024. PR #19399 later fixed a connection mix-up in it. Commit `ea954809ce` is ahead of both merge commits, so both changes are in it.

**Feature PR:** #18091, "scrape: support scraping targets via Unix Domain Sockets". It was merged on 2026-08-06 as `c5fa89db`.
- Its description says "Fixes Fixes #12024", and its release note reads "[FEATURE] Scraping: Add support for scraping targets via Unix Domain Sockets. #12024".
- I did not open #12024 itself. The issue number comes from the PR text.

**Fix PR:** #19399, "scrape: use a dedicated HTTP client per unix socket target". It was merged on 2026-08-14 as `05f9eb8b`.

**The mix-up:**
- The original feature passed the socket path to a shared HTTP client's `DialContext` through the request context.
- The HTTP transport keys idle pooled connections only on scheme and `host:port`, not on the socket path.
- So two targets with the same `__address__` but different `__scrape_unix_socket__` paths could reuse each other's pooled connections. A scrape could then hit the wrong endpoint.
- The same problem applied to a plain TCP target that shared that address.
- The fix gives each unix socket path its own scrape client. These clients are cached per scrape pool and rebuilt on reload.

I read all of this from the PR metadata and descriptions through the GitHub API. I did not read the code diffs.