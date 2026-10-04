The sync `Client` picks a transport per request by matching the URL against a sorted list of "mounts". The mounts are built once in `__init__` from the `proxy` argument or from environment variables. A URL that matches a mount whose value is `None`, or that matches nothing, goes direct through the default transport.

**1. Building the proxy map** (`httpx/_client.py:239-251`, `_get_proxy_map`)
- If `proxy` is passed, the map is `{"all://": Proxy(...)}`, so every URL is proxied.
- If `proxy` is `None` and `allow_env_proxies` is true, the map is built from `get_environment_proxies()`. Entries with a `None` URL stay `None`.
- If `proxy` is `None` and `allow_env_proxies` is false, the map is empty.
- `allow_env_proxies = trust_env and transport is None` (`_client.py:685`). A custom `transport=` therefore disables environment proxies.

**2. Environment proxies** (`httpx/_utils.py:30-70`, `get_environment_proxies`)
- It reads `urllib.request.getproxies()` and maps `http://`, `https://` and `all://` to the proxy URLs. A proxy without a scheme gets `http://` prepended.
- Each `NO_PROXY` entry becomes a pattern mapped to `None`, which means direct. For example, an IPv4 entry becomes `all://<host>`.
- `NO_PROXY=*` returns `{}`, so there is no proxying at all (`_utils.py:51-56`).

**3. Building the mounts** (`_client.py:688-716`)
- `self._transport` is the default direct `HTTPTransport`, or the user's `transport=` if one was given.
- `self._mounts` maps each `URLPattern(key)` to either `None` or a proxy `HTTPTransport(proxy=proxy)`. The `None` case covers NO_PROXY entries (`_client.py:697-710`, `_init_proxy_transport` at `:740-758`).
- User-supplied `mounts=` entries are merged in with `update`, so they override proxy entries that have the same pattern key (`:711-714`).
- The dict is then sorted (`:716`). I did not read `URLPattern`'s ordering code. I assume the sort puts more specific patterns first, so a specific NO_PROXY host wins over the general `all://` proxy.

**4. Per-request choice** (`_client.py:760-769`, `_transport_for_url`)
```python
for pattern, transport in self._mounts.items():
    if pattern.matches(url):
        return self._transport if transport is None else transport
return self._transport
```
- The first matching pattern wins. If its transport is `None`, the request uses the default direct transport. Otherwise it uses the proxy transport.
- If no pattern matches, the request uses the default transport.

`AsyncClient` has the same logic (`_client.py:1400-1479`).

**Uncertainty:** the ordering of `URLPattern` (its `__lt__`/priority logic) is in `_utils.py` after line 160, and I did not read it.