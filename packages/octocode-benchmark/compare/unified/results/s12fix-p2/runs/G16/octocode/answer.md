I did not check whether commit ea954809ce itself contains these two merges. Both PRs merged in August 2026, so I assumed they are in range.

- **Feature PR:** #18091, "scrape: support scraping targets via Unix Domain Sockets", by IngmarStein. It merged on 2026-08-06 as `c5fa89db085c9b3855d7916f2ade047066a7a318`.
- **Issue closed:** #12024. The PR body says "Fixes Fixes #12024" (the "Fixes" is duplicated in the body), and the release note references #12024. I did not open the issue or confirm it auto-closed.
- **Fix PR:** #19399, "scrape: use a dedicated HTTP client per unix socket target", by roidelapluie. It merged on 2026-08-14 as `05f9eb8b3b8e10b48c8f4153b0714dbe9bc9a630`.

**The mix-up:** the original feature passed the socket path to a shared HTTP client's `DialContext` through the request context. The HTTP transport keys idle pooled connections only on scheme and `host:port`. As a result, two targets with the same `__address__` but different `__scrape_unix_socket__` paths could reuse each other's pooled connections and scrape the wrong endpoint. A plain TCP target sharing that address could hit the same problem.

#19399 gives each unix socket path its own scrape client. The client is cached per scrape pool and rebuilt on reload. The PR body says its first commit adds a failing test for the bug and the second commit fixes it.