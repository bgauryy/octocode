I couldn't verify this at commit `b5addb64f0`. The Octocode tools aren't available in this session, so I can't read the repo. The account below is from my memory of httpx's `_client.py` and has no line numbers. Names and behavior may differ at that commit, so please check it before relying on it.

**Direct answer:** the sync `Client` picks a transport per request by matching the request URL against a table of "mounts". Each mount pairs a URL pattern with a transport. A matching pattern with a proxy transport sends the request through the proxy. A matching pattern mapped to `None`, or no match, falls back to the default direct transport.

**Mechanism (from memory):**
1. **Building the proxy map.** `Client.__init__` calls `_get_proxy_map(proxy, allow_env_proxies)`.
   - If an explicit `proxy=` is given, the map is `{"all://": Proxy(...)}`, so every request uses the proxy.
   - Otherwise, if `allow_env_proxies` is true, the map comes from `get_environment_proxies()`. This is true only when `trust_env` is set and neither `transport` nor `app` is given.
   - If neither applies, the map is empty and everything connects directly.
2. **Environment variables.** `get_environment_proxies()` reads `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` and `NO_PROXY`, via `urllib.request.getproxies()`. Each `NO_PROXY` entry is mapped to `None`, which means "direct".
3. **Mounts.** For each map entry, `__init__` creates a `URLPattern(key)`. The value is `None` if the proxy is `None`, otherwise `_init_proxy_transport(proxy, ...)`, which builds an `HTTPTransport(proxy=...)`. The default `self._transport` is built separately by `_init_transport` and connects directly. The mounts are then sorted so more specific patterns come first.
4. **Per-request choice.** `_transport_for_url(url)` loops over `self._mounts`. The first pattern where `pattern.matches(url)` is true wins. It returns `self._transport` if that mount's value is `None`, and the proxy transport otherwise. If no pattern matches, it returns `self._transport`, the direct one.

**Uncertainty:** I haven't confirmed any of this at `b5addb64f0`, including function names, the `proxy=` versus older `proxies=` argument, and the sort order of mounts. If you re-enable the Octocode tools, I can confirm it and add `path:line` citations.