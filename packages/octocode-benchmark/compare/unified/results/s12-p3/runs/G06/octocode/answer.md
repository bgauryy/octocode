The sync `Client` picks a transport per request by matching the request URL against URL patterns that were registered when the client was constructed. If no pattern matches, the request goes direct. All line numbers below are in `httpx/_client.py` at b5addb64f0.

**1. Building the proxy map at construction** (`_get_proxy_map`, lines 239-251, called at line 686)
- With an explicit `proxy=` argument, the map is `{"all://": Proxy(...)}` (lines 249-251). Every URL is proxied.
- With no `proxy` and `trust_env and transport is None`, the map comes from `get_environment_proxies()` (lines 243-247). That covers the `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` and `NO_PROXY` environment variables. A `None` value in that map means "don't proxy this pattern", for example a `NO_PROXY` entry.
- Otherwise the map is empty (line 248). The environment is ignored when a custom `transport=` is passed (line 685).

**2. Mounting transports** (lines 697-716)
- `self._transport` is the default direct connection pool, created by `_init_transport` (line 688).
- `self._mounts` maps each `URLPattern(key)` to one of two things (lines 697-710):
  - `None` when the proxy value is `None`, meaning direct.
  - A proxy transport built by `_init_proxy_transport` otherwise.
- User-supplied `mounts=` entries are merged in and override the proxy map (lines 711-714).
- The mounts are then sorted (line 716), so more specific patterns are checked before general ones.

**3. Choosing at request time** (`_transport_for_url`, lines 760-769, called at line 1005)
```python
for pattern, transport in self._mounts.items():
    if pattern.matches(url):
        return self._transport if transport is None else transport
return self._transport
```
- The first matching pattern wins. A `None` transport falls back to the direct `self._transport`, which is how `NO_PROXY` exclusions work.
- If nothing matches, it returns `self._transport`, so the connection is direct.

**Uncertainty:** I didn't read the `URLPattern` matching or sorting code, `get_environment_proxies`, or `_init_proxy_transport`. My search for `get_environment_proxies` in `httpx/_utils.py` came back empty, so it probably lives elsewhere. The sorted-specificity point is inferred from the `sorted` call at line 716.