**Short answer:** `Client` picks a transport for each request by matching the request URL against a sorted table of URL patterns (`self._mounts`). If a pattern matches and its transport is a proxy transport, the request goes through the proxy. If nothing matches, or the matching entry is `None`, it uses the default direct transport `self._transport`.

All line numbers below are in `httpx/_client.py` at b5addb64f0. I read the file through the GitHub API, so I haven't run any of this.

**1. Building the proxy map at construction** (`_get_proxy_map`, lines 239–251)
- If an explicit `proxy=` argument is given, the map is `{"all://": Proxy(...)}`, so every URL is proxied.
- If `proxy` is `None` and env proxies are allowed, the map comes from `get_environment_proxies()` (defined at `httpx/_utils.py:30`). A `None` value for a key means "don't proxy this pattern".
- If `proxy` is `None` and env proxies are not allowed, the map is empty and nothing is proxied.
- Env proxies are allowed only when `trust_env and transport is None` (line 685). A custom `transport=` therefore disables env-proxy lookup.

**2. Building the mounts** (`Client.__init__`, lines 686–716)
- `self._transport` is the default direct transport (`_init_transport`).
- `self._mounts` maps `URLPattern(key)` to `None` when the proxy value is `None`, or to a proxy transport from `_init_proxy_transport` otherwise.
- User-supplied `mounts=` entries are merged in afterwards (lines 711–714), so they override proxy entries with the same pattern.
- The dict is then sorted (line 716). `URLPattern` ordering puts more specific patterns before general ones, though I did not read `URLPattern` itself to confirm this.

**3. Per-request choice** (`_transport_for_url`, lines 760–769, called from the send path at line 1005)
- It iterates the sorted `_mounts` and returns the first pattern that `matches(url)`.
- It returns `self._transport` (direct) if that entry's transport is `None`, and the proxy transport otherwise.
- If no pattern matches, it returns `self._transport`.

**Uncertainty:** I did not read `get_environment_proxies` (how it handles `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` and `NO_PROXY`). I also did not read `URLPattern.matches` or its sort order, so I haven't verified the exact matching and priority rules.