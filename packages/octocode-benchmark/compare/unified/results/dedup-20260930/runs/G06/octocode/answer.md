**Short answer:** The sync `Client` picks a transport per request by matching the request URL against a dict of URL patterns (`self._mounts`). A matching pattern routes the request to that pattern's proxy transport. If nothing matches, it uses the default direct-connection transport. I read the code at b5addb64f0 in `httpx/_client.py`.

**1. Per-request decision**
- `_send_single_request` calls `transport = self._transport_for_url(request.url)` (line 1005). It then calls `transport.handle_request(request)`.
- `Client._transport_for_url` (def at line 760) loops over `self._mounts.items()`.
  - For the first `pattern` where `pattern.matches(url)` is true, it returns `self._transport` if that mount's transport is `None`, otherwise the mounted transport.
  - If no pattern matches, it returns `self._transport`.
- `self._transport` is the standard direct connection pool. A mount whose value is `None` means "bypass the proxy and go direct".

**2. How `_mounts` is built in `Client.__init__` (roughly lines 680–720)**
- `allow_env_proxies = trust_env and transport is None`. Environment proxies are therefore ignored when `trust_env=False` or when a custom `transport` is given.
- `proxy_map = self._get_proxy_map(proxy, allow_env_proxies)` (def at line 239):
  - If `proxy` is `None` and env proxies are allowed, the map comes from `get_environment_proxies()`. The proxy env variables are read through that function, which I did not open. Each entry becomes `Proxy(url=url)`, or `None` when the env entry has no URL (for example a `NO_PROXY` exclusion).
  - If `proxy` is `None` and env proxies are not allowed, the map is `{}`, so every request goes direct.
  - If `proxy` is given, the map is `{"all://": Proxy(...)}`, so every request goes through it.
- For each entry in `proxy_map`, `_mounts` gets a `URLPattern(key)` mapped to a transport. The transport is `None` for direct entries, otherwise `self._init_proxy_transport(...)`. That method returns an `HTTPTransport(..., proxy=proxy)`. I did not read the exact line where this conditional is written; it sits in the omitted lines 693–702 of the output.
- Any user-supplied `mounts=` are merged in with `self._mounts.update(...)`.
- The final step is `self._mounts = dict(sorted(self._mounts.items()))`. This sorts the patterns, which fixes the order the first-match loop checks them in. The sort order comes from `URLPattern`, which I did not inspect.

**Uncertainty:** I did not read `URLPattern.matches`, its ordering logic, or `get_environment_proxies`. The exact line numbers for the `__init__` mount-building block are approximate, since the tool omitted parts of that range. The async client has the same logic in `AsyncClient`.