The sync `Client` chooses per request by matching the request URL against a sorted table of URL patterns (`self._mounts`). If a pattern matches, it uses that pattern's transport, which is the proxy transport for proxy patterns. If nothing matches, it uses the default direct transport. All line numbers below are in `httpx/_client.py` at b5addb64f0, except the `_utils.py` ones.

**1. Dispatch at send time**
- `_send_single_request` calls `self._transport_for_url(request.url)` (`_client.py:1005`).
- `_transport_for_url` (`:760-770`) loops over `self._mounts.items()`.
- For the first `pattern.matches(url)` that is true, it returns `self._transport` if the mapped transport is `None`, otherwise the mapped transport.
- If no pattern matches, it returns `self._transport`, the direct connection pool.
- A `None` mount therefore means "connect directly", which is how `NO_PROXY`-style exclusions work.

**2. Building the mounts (`Client.__init__`, `:685-716`)**
- `allow_env_proxies = trust_env and transport is None` (`:685`).
- `proxy_map = self._get_proxy_map(proxy, allow_env_proxies)` (`:686`).
- `_get_proxy_map` (`:239-251`) works as follows:
  - If the `proxy=` argument is given, it returns `{"all://": Proxy}`, so every URL is proxied.
  - If `proxy` is `None` and env proxies are allowed, it builds the map from `get_environment_proxies()` (`_utils.py:30`). A `None` value for a key means that pattern is excluded from proxying.
  - Otherwise it returns `{}`, so everything goes direct.
- Each map entry becomes `URLPattern(key): None` if the proxy is `None`, or else `self._init_proxy_transport(...)` (`:697-710`).
- User-supplied `mounts=` are then merged over these, so they override matching keys (`:711-714`).
- Finally, `self._mounts = dict(sorted(self._mounts.items()))` (`:716`). `URLPattern.__lt__` (`_utils.py:222`) defines the order, which determines which pattern is checked first. I did not read the ordering logic, so I can't say exactly how it ranks specificity.

**Uncertainty:** I read `URLPattern.matches` and `get_environment_proxies` only by their locations (`_utils.py:192` and `:30`), not their bodies. I haven't verified how they handle schemes, hosts, ports and `NO_PROXY` rules.