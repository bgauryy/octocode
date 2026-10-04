The sync `Client` picks the transport per request by matching the request URL against a table of mounted transports. A match returns a proxy transport, or the direct pool if the match is a `None` entry. No match also returns the direct pool. All line numbers are in `httpx/_client.py` at b5addb64f0.

**At construction (`Client.__init__`)**
- `allow_env_proxies = trust_env and transport is None` (line 685). Environment proxies are ignored if you pass a custom `transport=` or set `trust_env=False`.
- `proxy_map = self._get_proxy_map(proxy, allow_env_proxies)` (line 686).
- `_get_proxy_map` (lines 239–251) builds that map in one of three ways:
  - If an explicit `proxy` is given, the map is `{"all://": Proxy(...)}`, so every URL goes through it.
  - If `proxy` is `None` and env proxies are allowed, it uses `get_environment_proxies()`. Each key is a URL pattern, and the value is `None` (no proxy for that pattern) or a `Proxy(url=...)`.
  - Otherwise the map is empty.
- `self._transport` is the direct `HTTPTransport` connection pool, built by `_init_transport` (line 688).
- `self._mounts` is a `dict[URLPattern, BaseTransport | None]` (lines 697–710). A `None` proxy gives a `None` mount, meaning direct. Otherwise `_init_proxy_transport` builds an `HTTPTransport(..., proxy=proxy)` (lines 740–758).
- User-supplied `mounts=` entries are merged in with `self._mounts.update(...)` (lines 711–714), so they override proxy-map entries with the same pattern.
- `self._mounts = dict(sorted(self._mounts.items()))` (line 716) orders the patterns. I did not read `URLPattern`'s ordering, so I'm assuming it puts more specific patterns first.

**At request time**
- `_send_single_request` calls `transport = self._transport_for_url(request.url)` (line 1005), then `transport.handle_request(request)` (line 1014).
- `_transport_for_url` (lines 760–769) loops over `self._mounts.items()`. The first pattern where `pattern.matches(url)` is true returns `self._transport` if that mount is `None`, otherwise the mounted proxy transport. If nothing matches, it returns `self._transport`.

**Not verified**
- I did not read `get_environment_proxies` (a code search for it in `httpx/_utils.py` returned nothing, so it likely lives elsewhere).
- I did not read `URLPattern.matches` or its sort order.