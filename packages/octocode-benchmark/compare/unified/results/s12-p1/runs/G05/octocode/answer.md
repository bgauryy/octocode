A Session follows redirects in `SessionRedirectMixin.resolve_redirects`, a generator in `src/requests/sessions.py:186-307`. It copies the previous prepared request on each hop, adjusts the copy, and sends it with `allow_redirects=False`. I did not read the code in `Session.send` that calls this generator, so how it is triggered is unverified. All line numbers below are at commit 611c6162cbc4.

**Loop mechanics**
- `get_redirect_target` returns the `Location` header only if `resp.is_redirect`. It re-encodes the header from latin1 to UTF-8 (`:134-152`).
- While a redirect URL exists, each iteration starts with `prepared_request = req.copy()` (`:204-205`).
- It appends the response to `hist` and sets `resp.history` (`:208-209`).
- It reads `resp.content` so the socket is consumed, and falls back to `resp.raw.read(decode_content=False)` on decode or chunk errors (`:211-214`).
- It raises `TooManyRedirects` once `len(resp.history) >= self.max_redirects` (`:216-219`).
- It then closes the response to release the connection (`:222`).
- It sends the new request with `self.send(..., allow_redirects=False)`, so the loop here does the following (`:292-301`).
- The loop takes the next target from `get_redirect_target(resp)` and yields the response (`:306-307`).
- With `yield_requests=True` it yields the prepared request instead of sending it (`:289-290`).

**What changes between hops**
1. **URL (`:224-245`)**
   - A scheme-relative `//host` URL gets the previous response's scheme.
   - If the new URL has no fragment, the previous fragment is carried over.
   - A relative `Location` is joined to `resp.url`.
   - The URL is percent-requoted.
2. **Method (`rebuild_method`, `:370-392`)**
   - 303 becomes GET unless the method is HEAD.
   - 302 becomes GET unless the method is HEAD.
   - A POST that gets a 301 becomes GET.
3. **Body and headers (`:249-258`)**
   - For any status other than 307 or 308, `Content-Length`, `Content-Type` and `Transfer-Encoding` are removed, and `body` is set to `None`.
   - On 307 and 308 the body is kept. If it was a consumed file-like object, it is rewound with `rewind_body` (`:275-284`).
4. **Cookies (`:260-269`)**
   - The old `Cookie` header is dropped.
   - Cookies from the response are extracted into the request's cookie jar and merged with the session's cookies.
   - The `Cookie` header is then rebuilt with `prepare_cookies`.
   - After the next response arrives, its cookies are also stored in `self.cookies` (`:303`).
5. **Auth (`rebuild_auth`, `:309-332`)**
   - `Authorization` is deleted if `should_strip_auth` says the hostname changed. A port or scheme change also strips it, except http to https on the default ports and same-scheme default-port cases (`:154-184`).
   - If `trust_env` is set, `.netrc` credentials for the new URL are re-applied.
6. **Proxies (`rebuild_proxies`, `:334-368`)**
   - Proxy settings are re-resolved for the new URL, so `NO_PROXY` can strip them.
   - `Proxy-Authorization` is removed. It is re-added from the proxy URL's credentials only when the scheme is not https.

**Uncertainty:** I read only `sessions.py` lines 186-394. I did not check `Response.is_redirect`, `rewind_body` or `Session.send`.