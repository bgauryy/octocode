A Session follows redirects in `SessionRedirectMixin.resolve_redirects`, a generator in `src/requests/sessions.py`. It sends each hop through `self.send(..., allow_redirects=False)`. All line numbers below are at 611c6162cb.

**How it follows redirects**
- `Session.send` pops `allow_redirects` (default True) at line 773. If it is set, it runs `gen = self.resolve_redirects(r, request, **kwargs)` and collects the responses into `history` (lines 802-805). The last response becomes `r`, and the earlier ones go into `r.history` (lines 809-815).
- If `allow_redirects` is False, it still builds the next request with `yield_requests=True` (line 821) but does not send it. This makes `r.next` available.
- `get_redirect_target` (lines 134-151) returns the `Location` header only if `resp.is_redirect`. It re-encodes the header from latin1 to UTF-8.
- The loop `while url:` (line 203) does the following on each hop:
  - It copies the previous prepared request with `req.copy()`.
  - It records the response in `resp.history`.
  - It reads `resp.content` so the connection can be released, then calls `resp.close()`.
  - It raises `TooManyRedirects` once `len(resp.history) >= self.max_redirects` (lines 216-219). The default is `DEFAULT_REDIRECT_LIMIT`, set at line 488.
  - It sends the new request with `allow_redirects=False` (line 299), reads the next target (line 305), and yields the response.

**What changes between hops** (all applied to the copied `PreparedRequest`)
1. **URL** (lines 221-250):
   - A `//host` location gets the previous scheme.
   - A fragment-less location inherits the previous fragment.
   - A relative location is joined with `resp.url` via `urljoin`.
   - The result is run through `requote_uri`.
2. **Method** (`rebuild_method`, lines 370-393):
   - A 303 becomes GET, unless the method is HEAD.
   - A 302 becomes GET, unless the method is HEAD.
   - A 301 on a POST becomes GET.
   - 307 and 308 keep the method.
3. **Body and headers** (lines 254-263):
   - For any status other than 307 or 308, it removes `Content-Length`, `Content-Type` and `Transfer-Encoding`, and sets `body = None`.
   - For 307 and 308 the body is kept, and a rewindable file-like body is rewound with `rewind_body` (lines 285-286).
4. **Cookies** (lines 265-278):
   - It drops the `Cookie` header.
   - It extracts cookies from the redirect response into the request's jar and merges in `self.cookies`.
   - It re-prepares the `Cookie` header from the merged jar.
5. **Auth** (`rebuild_auth`, lines 309-331):
   - It deletes `Authorization` when `should_strip_auth` (lines 154-184) says so. That is true if the hostname changed, or if the scheme or port changed. The exceptions are an http→https move on the default ports and a same-scheme move between default ports, where it is kept.
   - It then re-applies `.netrc` auth for the new URL if `trust_env` is set.
6. **Proxies** (`rebuild_proxies`, lines 334-367):
   - It recomputes the proxies for the new URL via `resolve_proxies`, so NO_PROXY is honoured.
   - It deletes any `Proxy-Authorization` header.
   - It sets a new `Proxy-Authorization` header if the proxy URL has credentials and the scheme is not https.

Not verified: I did not read the `send` body beyond the lines listed, and I did not run the code.