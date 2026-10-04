I did not check whether commit ea954809ce itself contains these merges. Both PRs merged in August 2026, and I assumed that is before the commit.

- **PR that added the feature:** #18091, "scrape: support scraping targets via Unix Domain Sockets", by IngmarStein. It merged on 2026-08-06 as merge commit c5fa89db085c9b3855d7916f2ade047066a7a318.
- **Issue it closed:** #12024. The PR body says "Fixes Fixes #12024", and its release note also references #12024. I did not open the issue to confirm GitHub closed it.
- **Later fix:** #19399, "scrape: use a dedicated HTTP client per unix socket target", by roidelapluie. It merged on 2026-08-14 as merge commit 05f9eb8b3b8e10b48c8f4153b0714dbe9bc9a630.
- **The mix-up:** Per #19399's description, the socket path travels to a shared HTTP client's `DialContext` through the request context. The transport keys idle pooled connections only on scheme and `host:port`. So two targets with the same `__address__` but different `__scrape_unix_socket__` paths could reuse each other's pooled connections and scrape the wrong endpoint. A plain TCP target sharing that address was affected the same way. The fix gives each unix socket path its own scrape client, cached per scrape pool and rebuilt on reload.

I took the mix-up description from the PR body and did not read the diff.