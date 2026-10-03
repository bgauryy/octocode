A Session follows redirects in `SessionRedirectMixin.resolve_redirects`, a generator that `Session` inherits. `Session.send` drives it. All line numbers are from `src/requests/sessions.py` at 611c6162cb. I read the file through the GitHub API, not a local checkout, and I did not run anything.

**How the loop works**
- `Session.send` calls `resolve_redirects(r, request, **kwargs)` and collects every yielded response when `allow_redirects` is true (`sessions.py:801-804`). It then puts the original response first in the history, pops the last response as the result and sets `r.history` (`:807-812`).
- `get_redirect_target` returns the `Location` header only if `resp.is_redirect`. It re-encodes the value from latin1 to UTF-8 (`:134-152`).
- Each hop starts from `prepared_request = req.copy()` (`:205`). The previous response is appended to the history and its body is consumed. Then `max_redirects` is checked, and exceeding it raises `TooManyRedirects` (`:208-219`). The connection is released with `resp.close()` (`:222`).
- Every hop is sent through `self.send(..., allow_redirects=False)` (`:292-301`), so the loop is iterative rather than recursive. Session cookies are extracted from each response (`:303`), and the next target is read from it (`:306`).
- With `allow_redirects=False`, `send` still calls `resolve_redirects(..., yield_requests=True)` once. It stores the prepared next request as `r._next` so `Response.next` works (`:818-823`). In that mode the generator yields the request and sends nothing (`:289-290`).

**What changes between hops**
1. **URL (`:225-245`)**
   - A `//host` location gets the scheme of the previous response.
   - A fragment-less location inherits the previous fragment.
   - A relative location is joined with `resp.url` via `urljoin`.
   - The result is run through `requote_uri`.
2. **Method (`rebuild_method`, `:370-392`)**
   - A 303 becomes GET unless the method is HEAD.
   - A 302 becomes GET unless the method is HEAD.
   - A 301 turns POST into GET.
   - Other cases keep the method, including 307 and 308.
3. **Body and headers (`:250-258`)**
   - For any status other than 307 or 308, the body is set to `None`. `Content-Length`, `Content-Type` and `Transfer-Encoding` are dropped.
   - For 307 and 308 the body is kept. If the body is rewindable, it is rewound with `rewind_body` (`:278-284`). A body that cannot be rewound raises `UnrewindableBodyError` (comment at `:275-277`).
4. **Cookies (`:260-269`)**
   - The `Cookie` header is removed.
   - Cookies are extracted from the response into the request's cookie jar and merged with the session jar. The `Cookie` header is then rebuilt with `prepare_cookies`.
5. **Auth (`rebuild_auth`, `:309-332`)**
   - `Authorization` is deleted if `should_strip_auth` returns true. That is the case when the hostname differs, or the scheme or port changes.
   - Exceptions: an http to https upgrade on the default ports, and same-scheme default-port usage (`:154-184`).
   - If `trust_env` is on, `.netrc` credentials for the new URL are applied.
6. **Proxies (`rebuild_proxies`, `:334-368`)**
   - Proxy settings are re-resolved for the new URL, which honours `NO_PROXY`.
   - `Proxy-Authorization` is always removed. It is re-added only for non-https schemes when the proxy URL contains credentials.

Stream, timeout, verify, cert and adapter kwargs are passed through unchanged on each hop (`:292-301`).

**Uncertainty**
- I did not check how `is_redirect` is defined (it is in `models.py`), so I can't say which status codes count as redirects.
- I did not check `DEFAULT_REDIRECT_LIMIT`, which sets the default `max_redirects` at `:488`.