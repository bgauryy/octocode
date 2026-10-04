**Short answer:** At construction, `Client` builds a table of URL patterns mapped to transports (`self._mounts`). For each request, `_transport_for_url` returns the first mount whose pattern matches the URL. If nothing matches, or the matching entry is `None`, it uses the direct transport `self._transport`. All line numbers below are in `httpx/_client.py` at b5addb64f0.

**Construction (`Client.__init__`)**
- `allow_env_proxies = trust_env and transport is None` (line 685). Environment proxies are therefore ignored if you pass a custom `transport` or set `trust_env=False`.
- `proxy_map = self._get_proxy_map(proxy, allow_env_proxies)` (line 686). `_get_proxy_map` is at lines 239-251 and works like this:
  - If `proxy` is given, the map is `{"all://": Proxy}`, so every URL is proxied.
  - If `proxy` is `None` and env proxies are allowed, the map comes from `get_environment_proxies()`, which is defined at `httpx/_utils.py:30`. I did not read its body. A `None` value means "no proxy for this pattern", for example a `NO_PROXY` entry.
  - Otherwise the map is empty.
- `self._mounts` (lines 697-710) maps `URLPattern(key)` to either `None` or a proxy-backed `HTTPTransport`. A `None` proxy gives a `None` mount, and anything else is built by `_init_proxy_transport` (lines 740-758) with `proxy=proxy`.
- User-supplied `mounts=` entries are merged over these (lines 712-714). The dict is then sorted (line 716) so more specific patterns are checked first. I did not read `URLPattern`'s ordering logic to confirm this.
- The direct transport is `self._transport`, created by `_init_transport`.

**Per request (`_transport_for_url`, lines 760-770)**
```python
for pattern, transport in self._mounts.items():
    if pattern.matches(url):
        return self._transport if transport is None else transport
return self._transport
```
- A match with a transport goes through that transport, which is the proxy.
- A match with `None` goes direct. This is how a `NO_PROXY` exclusion works.
- No match also goes direct.

The async client uses the same logic (lines 1399-1430 and 1474+).

**Not verified:** the body of `get_environment_proxies` (how it reads `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` and `NO_PROXY`) and the matching and sorting rules in `URLPattern`.