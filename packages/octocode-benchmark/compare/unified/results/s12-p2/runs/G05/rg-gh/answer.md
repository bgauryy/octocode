A `Session` follows redirects in `Session.send`, which hands the first response to the `SessionRedirectMixin.resolve_redirects` generator. That generator builds and sends one new request per hop. All line numbers below are from `src/requests/sessions.py` at 611c6162cb.

**How it follows redirects**
- `Session.send` pops `allow_redirects` (default `True`) at `sessions.py:773`. If it is true, it runs `gen = self.resolve_redirects(r, request, **kwargs)` and collects the responses into `history` (`:802-805`). It then makes the last response the result and sets its `.history` to the earlier ones (`:809-815`).
- With `allow_redirects=False`, the code at `:818-821` calls `resolve_redirects(..., yield_requests=True)`. I did not read past line 821, so I haven't confirmed what it does with that result. From context it looks like it stores the next request on the response, but I didn't verify that.
- `get_redirect_target` (`:134-152`) returns the `Location` header if `resp.is_redirect`, otherwise `None`. It re-encodes the header from latin1 to UTF-8.
- The loop in `resolve_redirects` (`:204-307`) runs while there is a target URL. On each pass it does the following:
  - It copies the previous request with `req.copy()` (`:205`).
  - It records history (`:208-209`).
  - It reads `resp.content`, falling back to `resp.raw.read(...)` if that raises (`:211-214`).
  - It raises `TooManyRedirects` once `len(resp.history) >= self.max_redirects` (`:216-219`). The default is `DEFAULT_REDIRECT_LIMIT`, set at `:488`.
  - It closes the response to release the connection (`:222`).
  - It sends the new request with `self.send(..., allow_redirects=False)` (`:292-301`). It then reads the next target and yields the response (`:306-307`).

**What changes between hops**
- **URL (`:225-245`)**:
  - A scheme-relative `//host` URL gets the previous scheme.
  - A fragment is carried over from the earlier URL if the new one has none (`:229-235`).
  - A relative `Location` is joined to `resp.url` with `urljoin`.
  - The URL is percent-encoded with `requote_uri`.
- **Method (`rebuild_method`, `:370-392`)**:
  - 303 becomes `GET`, unless the method is `HEAD`.
  - 302 becomes `GET`, unless the method is `HEAD`.
  - A `POST` after a 301 becomes `GET`.
- **Body and headers (`:249-258`)**: for any status other than 307 or 308, the body is set to `None`. The `Content-Length`, `Content-Type` and `Transfer-Encoding` headers are removed. For 307 and 308 the body is kept and rewound if possible (`:278-284`).
- **Cookies (`:260-269`)**:
  - The old `Cookie` header is dropped.
  - Cookies from the response are extracted into the jar and merged with the session's cookies.
  - `prepare_cookies` then rebuilds the header.
  - After each hop's response, its cookies are also added to `self.cookies` (`:303`).
- **Auth (`rebuild_auth`, `:309-332`)**:
  - The `Authorization` header is removed when `should_strip_auth` says so (`:324-327`).
  - `should_strip_auth` (`:154-184`) strips when the hostname changes or the port or scheme changes. It allows plain `http` to `https` on the default ports.
  - If `trust_env` is on, `.netrc` credentials for the new URL are applied (`:330-332`).
- **Proxies (`rebuild_proxies`, `:334-368`)**:
  - The proxy settings are recomputed for the new URL with `resolve_proxies`, which takes `NO_PROXY` into account.
  - `Proxy-Authorization` is dropped and set again from the proxy URL's credentials. It is set only when the new scheme is not `https`.

**Not verified:** I did not read `Session.send` beyond line 821, the adapters, or `Response.is_redirect`, so I can't say which status codes count as redirects.