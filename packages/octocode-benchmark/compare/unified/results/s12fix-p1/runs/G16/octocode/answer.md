The commit is dated 2026-09-26, after both PRs merged (2026-08-06 and 2026-08-14), so both are in its history. I checked that by date, not by ancestry.

- **PR that added the feature:** #18091, "scrape: support scraping targets via Unix Domain Sockets", by IngmarStein. It merged on 2026-08-06 as merge commit `c5fa89db085c9b3855d7916f2ade047066a7a318`.
- **Issue it closed:** #12024. The PR body says "Fixes Fixes #12024" (the word is doubled in the text), and its release note also cites #12024.
- **Later fix:** PR #19399, "scrape: use a dedicated HTTP client per unix socket target", by roidelapluie. It merged on 2026-08-14 as merge commit `05f9eb8b3b8e10b48c8f4153b0714dbe9bc9a630`.
- **The mix-up:**
  - The feature passed the socket path to a shared HTTP client's `DialContext` through the request context.
  - The transport keys idle pooled connections only on scheme and `host:port`.
  - So two targets with the same `__address__` but different `__scrape_unix_socket__` paths could reuse each other's pooled connections and scrape the wrong endpoint. The same could happen between a plain TCP target and a unix socket target that shared that address.
  - The fix gives each unix socket path its own scrape client. These clients are cached per scrape pool and rebuilt on reload.
  - The PR's first commit adds a failing test that shows the bug, and the second commit fixes it.

I took the mix-up description from PR #19399's body. I did not read its diff.