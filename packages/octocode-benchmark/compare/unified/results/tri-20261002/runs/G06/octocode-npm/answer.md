**Answer:** The decision is made per request by `Client._transport_for_url`, using a table of URL patterns built in `__init__`. The first pattern that matches the URL picks the transport. If nothing matches, the client uses its default direct transport.

All line references below are in `httpx/_client.py` at b5addb64f0. The line numbers I have are the matched-line anchors from the search output, so treat them as approximate.

**1. Building the proxy map at construction (`Client.__init__`, around line 686)**
- `allow_env_proxies = trust_env and transport is None`. Environment proxies are ignored if `trust_env=False` or if you pass a custom `transport`.
- `proxy_map = self._get_proxy_map(proxy, allow_env_proxies)` (around line 239) works like this:
  - If an explicit `proxy` is given, it returns `{"all://": Proxy(...)}`. Every request is proxied.
  - If `proxy` is `None` and env proxies are allowed, it builds the map from `get_environment_proxies()`. Each value becomes a `Proxy(url=...)`, or `None` for a no-proxy entry.
  - Otherwise it returns `{}`, so everything goes direct.
- `get_environment_proxies` (`httpx/_utils.py:30`) reads `urllib.request.getproxies()` and maps `http://`, `https://` and `all://` to the proxy URLs. Entries in `NO_PROXY` become patterns mapped to `None`, for example `all://*google.com`. A `NO_PROXY` containing `*` returns `{}`, which disables proxying entirely.
- In `__init__`, `self._transport` is the direct transport. `self._mounts` maps each `URLPattern(key)` to either `None` (direct) or a proxy transport made by `_init_proxy_transport` (an `HTTPTransport(..., proxy=proxy)`).
- User-supplied `mounts=` entries are merged in and override env or proxy entries with the same key. The dict is then sorted with `dict(sorted(self._mounts.items()))`, so the more specific patterns are checked first.

**2. Choosing the transport per request (`_transport_for_url`, around line 760)**
```python
for pattern, transport in self._mounts.items():
    if pattern.matches(url):
        return self._transport if transport is None else transport
return self._transport
```
- A match with a proxy transport sends the request through that proxy.
- A match with `None` (a `NO_PROXY` entry) sends it direct, via `self._transport`.
- No match also sends it direct.

**Not verified:** I didn't read `URLPattern.matches` or its sort order (in `httpx/_urlparse.py` or `_utils.py`). I'm relying on the sorting call to put specific patterns before general ones.