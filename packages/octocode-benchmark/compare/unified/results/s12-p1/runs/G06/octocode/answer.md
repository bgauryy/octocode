The sync `Client` decides per request by matching the request URL against a table of "mounts". It picks a proxy transport if a pattern matches, and the direct transport otherwise. All line numbers below are from `httpx/_client.py` at b5addb64f0.

**Setup in `Client.__init__`**
- `allow_env_proxies = trust_env and transport is None` (line 685). Environment proxies are used only if `trust_env` is on and no custom `transport=` was passed.
- `_get_proxy_map(proxy, allow_env_proxies)` (line 686, defined at 239–251) builds a map of URL pattern to proxy:
  - With an explicit `proxy=`, the map is `{"all://": proxy}`. A str or URL is wrapped in `Proxy`.
  - With no `proxy` but env proxies allowed, it is built from `get_environment_proxies()`. Each entry is `None` if the URL is `None`, otherwise `Proxy(url=url)` (lines 244–247).
  - Otherwise the map is empty.
- `self._transport` is the direct transport. It is the user's `transport=` if one was given, otherwise a plain `HTTPTransport` (`_init_transport`, lines 718–738).
- `self._mounts` (lines 697–710) maps `URLPattern(key)` to a transport. Each entry gets an `HTTPTransport(..., proxy=proxy)` from `_init_proxy_transport` (lines 740–758).
  - The mount value is `None` when the proxy is `None`. `None` means "go direct", and that is how an env entry with no proxy URL (a `no_proxy`-style exclusion, I assume) is expressed.
  - The condition `if proxy is None` at line 699 tests the loop variable, which shadows the `proxy` argument.
- User-supplied `mounts=` entries are merged in and override the proxy entries for the same key (lines 711–714).
- Finally `self._mounts = dict(sorted(self._mounts.items()))` (line 716) sorts the patterns. I did not read `URLPattern`'s ordering, so I can't say how it ranks specific patterns against `all://`.

**Per-request choice**
- `_send_single_request` calls `self._transport_for_url(request.url)` (line 1005). This runs for each request, and redirects are separate requests.
- `_transport_for_url` (760–769) loops over `self._mounts` in sorted order. On the first `pattern.matches(url)`, it returns `self._transport` if the mount value is `None`, otherwise that mount's transport (line 767). If nothing matches, it returns `self._transport`, which is the direct connection (line 769).

**Not verified**
- I did not read `get_environment_proxies()`, `URLPattern.matches`, or `URLPattern`'s sort order. The details of env-variable parsing, `no_proxy` handling and pattern precedence live there.