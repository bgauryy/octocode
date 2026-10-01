**Short answer:** The decision is made per request by URL-pattern matching against `Client._mounts`. `_mounts` is built once in `__init__` from the `proxy` argument or from environment variables. If a pattern matches and its mounted transport is a proxy transport, the request goes through the proxy. If it matches a `None` entry, or matches nothing, the request uses the direct `self._transport`. I read `httpx/_client.py` and `httpx/_utils.py` at b5addb64f0 through the GitHub API. There was no local checkout.

**1. Building the proxy map (at construction)**
- `Client.__init__` sets `allow_env_proxies = trust_env and transport is None` (`_client.py:685`). It then calls `self._get_proxy_map(proxy, allow_env_proxies)` (`_client.py:686`).
- `_get_proxy_map` (`_client.py:239-251`) has three cases:
  - An explicit `proxy` returns `{"all://": Proxy}`, so every request is proxied.
  - With no `proxy` and env proxies allowed, it uses `get_environment_proxies()`.
  - Otherwise it returns `{}`, so every request is direct.
- `get_environment_proxies` (`_utils.py:30-75`) reads `urllib.request.getproxies()` and handles the `http`, `https` and `all` schemes, giving keys like `http://`. Each `NO_PROXY` entry becomes a pattern mapped to `None`, such as `all://*example.com` or `all://localhost`. Entries are treated as follows:
  - A bare hostname becomes `all://*host`.
  - An IPv4 or IPv6 address, or `localhost`, is matched exactly.
  - `NO_PROXY=*` returns `{}` (`_utils.py:51-56`), which disables proxies entirely.

**2. Building the transports**
- `self._transport` is the direct connection pool, created by `_init_transport` (`_client.py:688`).
- `self._mounts` (`_client.py:697-711`) maps `URLPattern(key)` to `None` when the proxy value is `None`. Otherwise it maps to a transport from `_init_proxy_transport`.
- User-supplied `mounts=` entries are merged in and override matching keys (`_client.py:712-714`).
- `_mounts` is then sorted (`_client.py:716`). I didn't read `URLPattern`'s ordering code, so I'm inferring that this sort puts more specific patterns first.

**3. Per-request choice**
- `_transport_for_url` (`_client.py:760-769`) loops over `_mounts`. On the first `pattern.matches(url)` it returns `self._transport` if the mounted value is `None`, and the proxy transport otherwise. If nothing matches, it returns `self._transport`.
- `_send_single_request` calls it with `request.url` (`_client.py:1005`).

**Caveats**
- If you pass a custom `transport=`, env proxies are ignored (`_client.py:685`). An explicit `proxy=` or `mounts=` still applies.
- I didn't open `URLPattern.matches` in `_utils.py`, so I haven't verified its matching and sort-order details.
- `AsyncClient` mirrors this logic at `_client.py:1399-1430` and `1474`.