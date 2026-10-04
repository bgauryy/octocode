**Short answer:** At construction time, `Client` builds a table of URL patterns (`self._mounts`). On each request it picks the first pattern that matches the URL. A matching pattern with a proxy transport sends the request through that proxy. A matching pattern whose value is `None`, or no match at all, uses the direct transport. All line numbers below are in `httpx/_client.py` at b5addb64f0.

**1. Building the proxy map: `_get_proxy_map`, lines 239–251**
- If you pass `proxy=...`, the map is `{"all://": Proxy}`, so every URL is proxied (lines 249–251).
- If `proxy` is `None` and `allow_env_proxies` is true, the map comes from `get_environment_proxies()`. This is defined in `httpx/_utils.py:30`, and I did not read its body. In the map, `None` means "no proxy for this pattern" (lines 242–247).
- Otherwise the map is empty (line 248).
- `allow_env_proxies = trust_env and transport is None` (line 684). Environment proxies are therefore used only when `trust_env` is on and you did not pass a custom `transport`.

**2. Building the mounts: `Client.__init__`, lines 686–716**
- `self._transport` is the default direct transport (line 688).
- `self._mounts` maps `URLPattern(key)` to either `None` (when the proxy is `None`) or a transport from `_init_proxy_transport` (lines 697–710).
- User-supplied `mounts=` entries are merged in afterwards and override the proxy-derived ones for the same pattern (lines 711–714).
- `self._mounts` is then sorted (line 716). The sort order comes from `URLPattern`, which I did not read.

**3. Choosing a transport per request: `_transport_for_url`, lines 760–769**
- It iterates `self._mounts` in sorted order and returns the first match (lines 765–767).
- If the matching transport is `None`, it returns `self._transport`, so the request connects directly. This is how a `no_proxy`-style entry works.
- If nothing matches, it also returns `self._transport` (line 769).

**Uncertainty:** I did not open `URLPattern.matches` or the sort ordering in `httpx/_utils.py`. Which pattern wins when several match, for example a specific host against `all://`, depends on that code. I also did not verify how `get_environment_proxies` parses the environment variables.