**Answer:** The sync `Client` decides per request by matching the request URL against URL patterns in `self._mounts`. Each pattern maps to either a proxy transport or `None`. `None` means "use the direct transport". If nothing matches, the request also goes direct. All of this is in `httpx/_client.py` at commit b5addb64f0.

**1. Building the proxy map (`Client._get_proxy_map`, lines 239–250).**
- If you pass `proxy=`, the map is `{"all://": Proxy(...)}`. A `str` or `URL` is wrapped in `Proxy`, and every URL is proxied.
- If `proxy` is `None` and `allow_env_proxies` is true, the map is built from `get_environment_proxies()`. An entry whose URL is `None` becomes `None`, which is how a `no_proxy`-style exclusion would appear (my inference, since I did not read `get_environment_proxies`).
- If `proxy` is `None` and env proxies are not allowed, the map is empty (`{}`).

**2. Client construction (lines 685–711).**
- `allow_env_proxies = trust_env and transport is None`, so a custom `transport=` disables environment proxies.
- `self._transport` is the direct transport. It is the user's `transport` if one was given, otherwise a plain `HTTPTransport`.
- `self._mounts` maps `URLPattern(key)` to `None` when the proxy entry is `None`. Otherwise it maps to a new `HTTPTransport(..., proxy=proxy)` from `_init_proxy_transport` (lines 740–762).
- User-supplied `mounts=` entries are then merged in with `update`, so they override proxy entries that use the same pattern.
- Finally `self._mounts = dict(sorted(self._mounts.items()))` reorders the patterns. I did not read `URLPattern`, so I did not confirm the sort order or that it puts more specific patterns first.

**3. Per-request choice (`Client._transport_for_url`, lines 763–771).**
```python
for pattern, transport in self._mounts.items():
    if pattern.matches(url):
        return self._transport if transport is None else transport
return self._transport
```
- The first matching pattern wins.
- A matching pattern with a `None` transport gives the direct `self._transport`.
- A matching pattern with a proxy transport sends the request through that proxy.
- No match gives the direct `self._transport`.

**Not verified:** I did not read `URLPattern.matches`, its sort ordering, or `get_environment_proxies`. I also did not read the code that calls `_transport_for_url`.