**Short answer:** `Session.send()` sends the first request with redirects off, then calls `resolve_redirects()`. That generator copies the previous `PreparedRequest` for each hop, edits the copy, and sends it with `allow_redirects=False`. All line numbers below are in `src/requests/sessions.py` at 611c6162cb.

**How it follows redirects**
- `send()` pops `allow_redirects`, which defaults to True (`sessions.py:773`). If it is set, it builds `gen = self.resolve_redirects(r, request, **kwargs)` and collects the responses into `history` (`:802-805`, with `r.history = history` at `:815`).
- If redirects are off, `send()` calls `resolve_redirects(..., yield_requests=True)` and stores the first yielded request as `r._next`, which backs `Response.next` (`:817-821`). The `:817-821` read ended partway through the call, so that detail is from a truncated read.
- `get_redirect_target()` returns the `Location` header only if `resp.is_redirect`. It re-encodes the header from latin1 and decodes it as UTF-8 (`:134-152`).
- Each loop iteration in `resolve_redirects` (`:204-307`) does the following:
  - It copies the previous request with `req.copy()` (`:205`).
  - It appends the previous response to `hist` and sets `resp.history` (`:208-209`).
  - It reads `resp.content` so the connection can be released. If decoding fails, it reads the raw stream instead (`:211-214`).
  - It raises `TooManyRedirects` when `len(resp.history) >= self.max_redirects` (`:216-219`), then calls `resp.close()` (`:222`).
  - It sends the new request via `self.send(..., allow_redirects=False)` (`:292-301`) and calls `get_redirect_target()` on the result for the next iteration (`:306`).
- With `yield_requests=True` it yields the prepared request instead of sending it (`:289-290`).

**What changes between hops**
1. **URL (`:225-245`):**
   - A `//host` location gets the previous response's scheme.
   - If the new URL has no fragment, the previous fragment is carried over.
   - A relative location is joined to `resp.url` with `urljoin`, and the URL is percent-encoded with `requote_uri`.
2. **Method (`rebuild_method`, `:370-392`):**
   - 303 becomes GET unless the method is HEAD.
   - 302 becomes GET unless the method is HEAD.
   - 301 turns POST into GET.
   - 307 and 308 keep the method.
3. **Body and headers (`:249-258`):**
   - For any status other than 307 or 308, it removes `Content-Length`, `Content-Type` and `Transfer-Encoding`, and sets `body = None`.
   - For 307 and 308 the body is kept. If the body is file-like and rewindable, `rewind_body()` is called (`:275-284`).
4. **Cookies (`:260-269`):**
   - It drops the `Cookie` header.
   - It extracts cookies from the redirect response into the request's jar and merges in the session cookies.
   - It re-prepares the `Cookie` header from that jar.
   - After each hop's response it also updates `self.cookies` (`:303`).
5. **Auth (`rebuild_auth`, `:309-332`):**
   - It deletes `Authorization` if `should_strip_auth()` says so. That is true when the hostname changes, or when the port or scheme changes.
   - There is an exception that allows http→https on the default ports (`:154-184`).
   - If `trust_env` is set and `.netrc` has credentials for the new URL, it re-applies them.
6. **Proxies (`rebuild_proxies`, `:334-368`):**
   - It recomputes the proxies for the new URL with `resolve_proxies`, so `NO_PROXY` is honored.
   - It drops any old `Proxy-Authorization` header.
   - For non-https schemes it sets a new one from credentials in the proxy URL.

**Not verified:** I did not read `send()` lines 777-798 or 806-814, or the body of `rewind_body` and `resolve_proxies`.