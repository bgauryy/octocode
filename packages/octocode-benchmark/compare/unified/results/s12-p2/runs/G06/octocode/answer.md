At b5addb64f0, the sync `Client` picks a transport per request by matching the request URL against a table of URL-pattern mounts. If a pattern matches and its mount holds a proxy transport, the request goes through the proxy. If nothing matches, or the matching mount is `None`, it connects directly through the default transport.

**Setup in `Client.__init__` (`httpx/_client.py`)**
- `_get_proxy_map` (lines 239-251) builds a dict of pattern to `Proxy`:
  - With no explicit `proxy`, and when env proxies are allowed, it uses `get_environment_proxies()`. A pattern whose env value is `None` maps to `None`, and the others become `Proxy(url=...)`. If env proxies aren't allowed, it returns `{}`.
  - With an explicit `proxy`, it returns `{"all://": proxy}`, so every URL matches.
- Env proxies are allowed only when `trust_env and transport is None` (line 685). Passing a custom `transport` therefore disables env proxies.
- `self._transport` is the default direct connection pool, built by `_init_transport` (lines 688-696).
- `self._mounts` (lines 697-710) maps `URLPattern(key)` to `None` when the proxy entry is `None`, or else to a transport from `_init_proxy_transport(...)`.
- User-supplied `mounts` are merged in afterwards and override entries with the same pattern (lines 711-714).
- The mounts are then sorted (line 716), which sets the order in which patterns are checked. I didn't read `URLPattern`'s ordering logic, so I can't say what the sort order is.

**Per-request decision: `_transport_for_url` (lines 760-769)**
```python
for pattern, transport in self._mounts.items():
    if pattern.matches(url):
        return self._transport if transport is None else transport
return self._transport
```
- The first matching pattern wins.
- A matching pattern with a `None` transport returns the direct transport. This is how env settings such as `NO_PROXY` exclude a host.
- No match returns the direct transport.

`send` calls this at line 1005: `transport = self._transport_for_url(request.url)`.

I didn't read `get_environment_proxies` or `URLPattern.matches`. How patterns are matched, and how env variables turn into patterns, is therefore not verified.