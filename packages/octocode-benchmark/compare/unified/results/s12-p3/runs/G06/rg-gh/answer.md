**Short answer:** `Client` builds a table of URL-pattern → transport mounts when it is constructed. For each request, `_transport_for_url` picks the first pattern that matches the URL. If nothing matches, or the matching entry is `None`, the request uses the default direct transport. Otherwise it uses a proxy transport. Line numbers below are from `httpx/_client.py` at b5addb64f0 (some cited lines from `_utils.py` have no numbers). I fetched the file through the GitHub API and counted its lines locally.

**1. Where the proxy settings come from (`Client.__init__`, lines 685–686)**
- `allow_env_proxies = trust_env and transport is None`, then `proxy_map = self._get_proxy_map(proxy, allow_env_proxies)`.
- `_get_proxy_map` (lines 239–251) handles three cases:
  - An explicit `proxy=` argument gives `{"all://": Proxy}`, so every request is proxied.
  - With no `proxy` and env proxies allowed, it returns the result of `get_environment_proxies()`.
  - With no `proxy` and env proxies not allowed, it returns `{}`.
- Env proxies are therefore ignored when `trust_env=False` or when a custom `transport=` is passed.

**2. How the environment is read (`httpx/_utils.py`, `get_environment_proxies`, starts at line 30)**
- It uses `urllib.request.getproxies()` to read the `http`, `https` and `all` proxy settings. These become the patterns `http://`, `https://` and `all://`.
- Each `NO_PROXY` entry becomes a pattern mapped to `None`, which means "go direct". For example, a plain domain becomes `all://*example.com`, and `localhost` and IP addresses get their own `all://` patterns.
- `NO_PROXY=*` returns `{}`, so no proxies are used at all.

**3. Building the mounts (lines 697–716)**
- `self._transport` is the default direct transport, created by `_init_transport`.
- `self._mounts` maps `URLPattern(key)` to either `None` (direct) or a proxy transport from `_init_proxy_transport` (defined at line 740).
- Any user-supplied `mounts=` entries are merged in with `update`, so they override entries with the same pattern.
- The dict is then sorted with `sorted(self._mounts.items())`, which orders the patterns by their own comparison rules so that more specific patterns are tried first. I did not read `URLPattern.__lt__` in `_utils.py`, so that ordering rule is inferred.

**4. Per-request choice (`_transport_for_url`, lines 760–769)**
```python
for pattern, transport in self._mounts.items():
    if pattern.matches(url):
        return self._transport if transport is None else transport
return self._transport
```
`None` entries come from `NO_PROXY`, so a matching `NO_PROXY` pattern sends the request direct even when a broader proxy pattern also exists.

`AsyncClient` has the same logic at about lines 1399–1430 and 1474.

**Uncertainty:** I did not read `URLPattern.matches` or its sort order, so how patterns are matched and ranked is not verified.