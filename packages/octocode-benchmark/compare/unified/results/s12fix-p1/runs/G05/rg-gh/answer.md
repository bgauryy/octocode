**Short answer:** `Session.send` sends the first request with redirects turned off. If `allow_redirects` is true, it hands the response to `SessionRedirectMixin.resolve_redirects`. That generator builds a modified copy of the previous request for each hop and sends it with `allow_redirects=False`. All lines below are from `src/requests/sessions.py` at 611c6162cb.

**How it follows redirects**
- `Session.send` pops `allow_redirects` at line 773. If it is true, it runs `gen = self.resolve_redirects(r, request, **kwargs)` and collects the responses into `history` (lines 802–805). It then makes the last response `r` and sets `r.history` to the earlier ones (lines 809–815).
- If `allow_redirects` is false, it still builds the next request with `resolve_redirects(..., yield_requests=True)` (line 821). That request is stored but not sent. I did not read past line 821, so I did not check what it is stored as.
- `get_redirect_target` (lines 134–152) returns the `Location` header if `resp.is_redirect`. It re-encodes the header from latin1 to UTF-8 and returns `None` otherwise.
- The loop in `resolve_redirects` (lines 204–307) runs as follows:
  - It copies the request with `req.copy()`.
  - It appends the response to `history`.
  - It reads `resp.content` to free the socket, then calls `resp.close()`.
  - It raises `TooManyRedirects` once `len(history) >= max_redirects` (lines 216–219).
  - It sends the new request with `self.send(..., allow_redirects=False)` (lines 292–301) and reads the next target (line 306).
  - It yields the response (line 307).

**What changes between hops**
- **URL (lines 224–245):**
  - A scheme-relative `//host` URL gets the previous response's scheme.
  - A fragment-less target inherits the previous fragment.
  - A relative `Location` is joined to `resp.url`.
  - The result is run through `requote_uri`.
- **Method (`rebuild_method`, lines 370–392):**
  - 303 becomes GET, unless the method is HEAD.
  - 302 becomes GET, unless the method is HEAD.
  - 301 turns a POST into GET.
  - Other statuses, including 307 and 308, keep the method.
- **Body and headers (lines 250–258):** for any status other than 307 or 308, it removes `Content-Length`, `Content-Type` and `Transfer-Encoding` and sets `body = None`. 307 and 308 keep the body. If the body is rewindable, `rewind_body` resets it (lines 278–284). If the body can't be rewound, `rewind_body` raises `UnrewindableBodyError`, per the comment at lines 275–277.
- **Cookies (lines 261–269, 303):**
  - It drops the `Cookie` header.
  - It extracts the response's cookies into the request's cookie jar and merges in the session's jar (`self.cookies`).
  - It re-prepares the cookie header.
  - After each send, it also extracts the response's cookies into `self.cookies`.
- **Auth (`rebuild_auth`, lines 309–332):**
  - It deletes `Authorization` if `should_strip_auth` says so. That is true when the hostname changes, or when the port or scheme changes.
  - Two exceptions keep the header: an http→https move on default ports (lines 164–170), and the same scheme with default ports (lines 172–181).
  - If `trust_env` is on, it applies any `.netrc` auth for the new URL.
- **Proxies (`rebuild_proxies`, lines 334–368):**
  - It re-resolves proxies for the new URL, which can apply `NO_PROXY` or environment settings.
  - It removes `Proxy-Authorization` and re-adds it from the proxy URL's credentials. It only does this for non-https schemes.
  - `resolve_redirects` carries the new proxy dict into the next hop.

**Uncertainty:** I did not read `Response.is_redirect` or `rewind_body`, so the rules for which statuses count as redirects and the exact rewind behavior are not verified here.