**Short answer:** `Client` builds a sorted table of URL patterns (`self._mounts`) when it is constructed. For each request, `_transport_for_url` returns the first pattern that matches the URL. If a pattern matches and its transport is a proxy transport, the request goes through that proxy. If the matching entry is `None`, or nothing matches, it uses the direct transport `self._transport`. I read the code through the GitHub API at `b5addb64f0161ff6bfe94c124ef76f6a1fba5254`. Line numbers below are in `httpx/_client.py` unless noted.

**1. Building the proxy map at construction**
- `allow_env_proxies = trust_env and transport is None`, then `proxy_map = self._get_proxy_map(proxy, allow_env_proxies)` (lines 685–686).
- `_get_proxy_map` (239–251) has three cases:
  - If an explicit `proxy=` is given, the map is `{"all://": Proxy(...)}`, so every URL is proxied.
  - If `proxy` is `None` and env proxies are allowed, the map comes from `get_environment_proxies()`.
  - Otherwise the map is empty, so all traffic is direct.
- `get_environment_proxies` is at `httpx/_utils.py:30-75`:
  - It reads `getproxies()` for the `http`, `https` and `all` schemes and maps them to keys like `http://`.
  - Each `NO_PROXY` entry becomes a pattern mapped to `None`, meaning "direct", such as `all://*example.com`.
  - `NO_PROXY=*` returns `{}`, which disables all proxying (`_utils.py:51-56`).

**2. Building the mounts**
- `self._mounts` (697–709) maps `URLPattern(key)` to `None` if the proxy is `None`, or else to a transport created by `_init_proxy_transport`.
- User-supplied `mounts=` entries override or extend this table (711–714).
- The table is then sorted with `dict(sorted(self._mounts.items()))` (716).

**3. Sort order**
- `URLPattern.priority` (`_utils.py:~200-210`) orders patterns from most to least specific:
  1. Patterns with a port come first.
  2. Longer hosts come next.
  3. Longer schemes come after that.
- `__lt__` compares on this priority.
- Because of this order, a `NO_PROXY` pattern like `all://*example.com` (longer host) is checked before the generic `http://` proxy pattern.

**4. Matching per request**
- `_send_single_request` calls `transport = self._transport_for_url(request.url)` (1005).
- `_transport_for_url` (760–770) loops over `self._mounts`. On the first `pattern.matches(url)`, it returns `self._transport` if the mapped transport is `None`, and the mapped transport otherwise. If nothing matches, it returns `self._transport`.
- `URLPattern.matches` (`_utils.py:192-203`) checks the scheme (empty for `all`), then the host regex, then the port.

**Caveat:** if you pass a custom `transport=`, env proxies are ignored (line 685). An explicit `proxy=` or `mounts=` is still applied.

I did not run anything. Everything above comes from reading the source. The `priority` line range in `_utils.py` is approximate.