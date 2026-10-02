**Answer:** `Client` decides per request by matching the request URL against a dict of URL patterns (`self._mounts`). The first matching pattern picks the transport. A matching pattern whose transport is `None` means "connect directly". Otherwise the request uses a proxy transport. All line numbers are in `httpx/_client.py` at b5addb64f0.

**Per-request choice** (`_transport_for_url`, 760–769):
- It loops over `self._mounts.items()`.
- For the first `pattern.matches(url)`, it returns `self._transport` if the mounted transport is `None`. Otherwise it returns the mounted transport, which is the proxy transport.
- If nothing matches, it returns `self._transport`, the default direct connection pool.
- `send` calls this at line 1005: `transport = self._transport_for_url(request.url)`.

**How `_mounts` is built** (`Client.__init__`, 685–716):
- Line 685: `allow_env_proxies = trust_env and transport is None`. Environment proxies are ignored if you pass a custom `transport` or set `trust_env=False`.
- Line 686: `proxy_map = self._get_proxy_map(proxy, allow_env_proxies)`. That function is at 239–251:
  - If you pass an explicit `proxy`, the map is `{"all://": proxy}`, so every URL goes through that proxy.
  - If `proxy` is `None` and env proxies are allowed, the map comes from `get_environment_proxies()` (imported from `._utils`). Each entry maps a pattern to a `Proxy`, or to `None` for a no-proxy exclusion.
  - Otherwise the map is empty.
- Lines 697–710: each pattern in the map becomes `URLPattern(key)`. A `None` entry stays `None`, meaning direct. Any other entry gets a transport from `_init_proxy_transport(...)`.
- Lines 711–714: user-supplied `mounts=` entries are merged in and override proxy-derived entries with the same pattern.
- Line 716: `self._mounts = dict(sorted(self._mounts.items()))`. Sorting makes the more specific patterns come first, so the first match wins. I did not read `URLPattern`'s ordering logic in `_utils`, so that is inferred from the sort and the first-match loop.

**Not verified:** I did not read `get_environment_proxies` or `URLPattern.matches`. How `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` and `NO_PROXY` become patterns is therefore not confirmed here.