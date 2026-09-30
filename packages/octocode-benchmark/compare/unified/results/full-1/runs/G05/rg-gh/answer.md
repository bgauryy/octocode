**Short answer:** `Session.send` sends the first request with redirects off. If `allow_redirects` is true, it hands the response to `SessionRedirectMixin.resolve_redirects`, which loops. Each hop copies the previous `PreparedRequest`, edits the copy, and resends it with `allow_redirects=False`. All line numbers below are in `src/requests/sessions.py` at 611c6162cb, which I read via the GitHub API.

**How the loop works**
- `Session.send` calls `gen = self.resolve_redirects(r, request, **kwargs)` at `:804` and collects the responses as `history` (`:805`). If `allow_redirects` is false, it only computes `r._next` via `yield_requests=True` (`:818-821`).
- `get_redirect_target` (`:134-152`) returns the `Location` header only when `resp.is_redirect`. It re-encodes the header from latin1 to UTF-8.
- `resolve_redirects` (`:186-307`) loops `while url:`.
  - Each iteration starts with `prepared_request = req.copy()` (`:199`).
  - It records `resp.history` (`:208`) and reads `resp.content` to drain the socket (`:211`).
  - It raises `TooManyRedirects` when `len(resp.history) >= self.max_redirects` (`:216-219`). The default is `DEFAULT_REDIRECT_LIMIT`, set at `:488`.
  - It closes `resp` to release the connection (`:223`).
  - It calls `self.send(req, ..., allow_redirects=False)` (`:293-300`), stores the cookies from the new response (`:302`), and reads the next target (`:305`).
  - It yields the response (`:306`).

**What changes between hops**
1. **URL (`:226-249`)**
   - A scheme-relative `//host` URL gets the previous response's scheme.
   - A fragment-less target inherits the previous fragment (`:232-237`).
   - A relative `Location` is resolved with `urljoin(resp.url, requote_uri(url))`. An absolute one is just requoted.
2. **Method (`rebuild_method`, `:370-393`)**
   - 303 becomes GET unless the method is HEAD.
   - 302 becomes GET unless the method is HEAD.
   - 301 turns POST into GET.
   - 307 and 308 keep the method.
3. **Body and headers (`:253-262`)**
   - For any status other than 307 or 308, it drops `Content-Length`, `Content-Type` and `Transfer-Encoding` and sets `body = None`.
   - For 307 and 308 the body is kept. If the body is a file-like object, it is rewound with `rewind_body` (`:279-280`). If the body can't be rewound, `UnrewindableBodyError` is raised, as the comment at `:274-276` indicates.
4. **Cookies (`:264-273`)**
   - It pops the old `Cookie` header.
   - It extracts cookies from the redirect response into the request's jar and merges in the session's cookies.
   - It calls `prepare_cookies` to rebuild the header.
5. **Auth (`rebuild_auth`, `:309-332`)**
   - It deletes `Authorization` when `should_strip_auth` (`:154-184`) says so.
   - `should_strip_auth` strips on a hostname change, or on a port or scheme change.
   - It allows http→https on the default ports, and treats default-port equivalents as unchanged.
   - If `trust_env` is set, it re-applies netrc credentials for the new URL.
6. **Proxies (`rebuild_proxies`, `:334-368`)**
   - It re-resolves proxies for the new URL, so `NO_PROXY` is honoured.
   - It removes `Proxy-Authorization` and re-adds it from the proxy URL's credentials, but only for non-https schemes.

**Uncertainty:** I read the code but did not run it. I did not check the `is_redirect` definition in `models.py`, so which status codes count as redirects is not covered here.