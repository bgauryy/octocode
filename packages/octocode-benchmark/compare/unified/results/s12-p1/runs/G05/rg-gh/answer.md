A `Session` follows redirects with a generator loop, `SessionRedirectMixin.resolve_redirects`. Each hop copies the previous prepared request, edits the copy, and re-sends it with `allow_redirects=False`. All line numbers below are from `src/requests/sessions.py` at 611c6162cb.

**How it follows redirects**
- `Session.send` pops `allow_redirects`, which defaults to `True` (`:773`). If it is set, `send` calls `self.resolve_redirects(r, request, **kwargs)` and collects the generator into `history` (`:802-805`).
- If `history` is non-empty, `send` inserts the first response at the front. It then pops the last response as the final `r` and sets `r.history = history` (`:809-815`).
- If `allow_redirects` is false, `send` still primes the generator with `yield_requests=True` (`:818-821`). This lets callers get the next request without sending it. I did not read past `:821`, so I haven't checked what `send` does with that request.
- `get_redirect_target` (`:134-152`) returns the `Location` header only if `resp.is_redirect`. It re-encodes the header from latin1 to utf8.
- The loop `while url:` (`:204`) works as follows:
  - It copies the request with `req.copy()` (`:205`).
  - It sets `resp.history` and appends the response to the history (`:208-209`).
  - It reads `resp.content` to consume the body, falling back to `resp.raw.read(decode_content=False)` on decoding errors (`:211-214`).
  - It raises `TooManyRedirects` if `len(resp.history) >= self.max_redirects` (`:216-219`).
  - It calls `resp.close()` to release the connection (`:222`).
  - It sends the new request via `self.send(..., allow_redirects=False)` (`:292-301`).
  - It reads the next target with `get_redirect_target` and yields the response (`:306-307`).

**What changes between hops**
- **URL (`:224-245`)**
  - A scheme-relative `//host` location gets the previous response's scheme.
  - The previous URL's fragment is carried over if the new location has none.
  - A relative location is joined to `resp.url` with `urljoin`.
  - The result is percent-encoded with `requote_uri`.
- **Method (`rebuild_method`, `:370-392`)**
  - A 303 becomes GET unless the method is HEAD.
  - A 302 becomes GET unless the method is HEAD.
  - A 301 on a POST becomes GET.
  - 307 and 308 keep the method.
- **Body and headers (`:250-258`)**
  - Unless the status is 307 or 308, `Content-Length`, `Content-Type` and `Transfer-Encoding` are dropped and `body` is set to `None`.
  - For 307 and 308 the body is kept. If it is a file-like object with a recorded position, it is rewound via `rewind_body` (`:278-284`).
- **Cookies (`:261-269`)**
  - The old `Cookie` header is removed.
  - Cookies from the response are extracted into the request's jar and merged with the session jar.
  - The `Cookie` header is then re-prepared from the merged jar.
  - After each send, response cookies also go into `self.cookies` (`:303`).
- **Auth (`rebuild_auth`, `:309-332`)**
  - `Authorization` is deleted if `should_strip_auth` (`:154-184`) says the origin changed. That means a different hostname, scheme or port.
  - The exception is an http→https upgrade on the default ports (or no explicit port), where `Authorization` is kept.
  - If `trust_env` is set, `.netrc` credentials for the new URL are applied.
- **Proxies (`rebuild_proxies`, `:334-368`)**
  - Proxies are re-resolved for the new URL, so `NO_PROXY` is respected.
  - `Proxy-Authorization` is removed. It is re-added only for non-HTTPS schemes when the proxy URL has credentials.

**Uncertainty:** I read only `sessions.py`. I did not read `Response.is_redirect`, `rewind_body` or `resolve_proxies`, so what they do internally is not verified.