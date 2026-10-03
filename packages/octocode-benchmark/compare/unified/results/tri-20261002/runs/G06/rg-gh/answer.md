**Short answer:** The sync `Client` picks a transport per request by matching the request URL against a sorted table of URL patterns (`self._mounts`). The first matching pattern wins. A match whose transport is `None`, or no match at all, means the request uses the default direct transport. A match with a proxy transport sends the request through that proxy.

**How it works** (all in `httpx/_client.py` at b5addb64f0, unless noted):

1. **Build the proxy map** (`_get_proxy_map`, lines 239-251):
   - If `proxy=` is given, the map is `{"all://": Proxy(...)}`, so every URL is proxied.
   - If `proxy` is `None` and env proxies are allowed, the map comes from `get_environment_proxies()` (`httpx/_utils.py:30`). A `None` URL in that result, such as a `NO_PROXY` entry, becomes `None` (direct).
   - Otherwise the map is empty.
2. **Gate on env proxies** (line 685): `allow_env_proxies = trust_env and transport is None`. Environment proxies are used only if `trust_env` is true and no custom `transport=` was passed.
3. **Build the mounts** (lines 697-716):
   - `self._transport` is the direct transport, created by `_init_transport`.
   - `self._mounts` maps `URLPattern(key)` to either `None` (direct) or a proxy transport from `_init_proxy_transport`.
   - User-supplied `mounts=` entries are merged in and override proxy entries with the same pattern.
   - The dict is then sorted (`sorted(self._mounts.items())`). That relies on `URLPattern.__lt__` (`_utils.py:222`), which I did not read, so I assume it puts more specific patterns first.
4. **Choose per request** (`_transport_for_url`, lines 760-770): it iterates `self._mounts` in order and, for the first `pattern.matches(url)` (`_utils.py:192`), returns `self._transport` if the mapped transport is `None`, otherwise the mapped transport. If nothing matches, it returns `self._transport`. This is called when sending a request at line 1005.

**Uncertainty:** I read the bodies of `URLPattern.matches` and `__lt__` only by location, not by content. The exact ordering and matching rules, and how `get_environment_proxies` parses `NO_PROXY`, are therefore not verified here.