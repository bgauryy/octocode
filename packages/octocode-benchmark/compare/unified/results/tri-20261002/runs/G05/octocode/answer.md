**Short answer:** `Session.send` sends the request and then, if `allow_redirects` is true, hands the response to the generator `SessionRedirectMixin.resolve_redirects`. For each hop, that generator copies the previous prepared request, edits the copy, and sends it with `allow_redirects=False`. All line numbers below are in `src/requests/sessions.py` at 611c6162cb.

**How it follows redirects**
- `Session.send` pops `allow_redirects` (default True) at line 773. If it is set, it calls `gen = self.resolve_redirects(r, request, **kwargs)` and collects `history = [resp for resp in gen]` (lines 802-805).
- It then inserts the original response at the front of the list and pops the last response as the result. The earlier ones become `r.history` (lines 810-815).
- If `allow_redirects` is false, `send` instead stores the next hop on the response as `r._next` by calling `resolve_redirects(..., yield_requests=True)` once (lines 817-824). In that mode the generator yields the prepared request and does not send it (lines 289-290).
- `get_redirect_target` (lines 134-152) returns the `Location` header, but only if `resp.is_redirect`. It re-encodes the value from latin1 to utf8.
- The loop `while url:` (line 204) does the following on each pass:
  - It builds `prepared_request = req.copy()` (line 205).
  - It appends the response to `hist` and sets `resp.history`.
  - It reads `resp.content` so the socket is consumed, and it falls back to `resp.raw.read(decode_content=False)` on certain errors (lines 211-214).
  - It raises `TooManyRedirects` if `len(resp.history) >= self.max_redirects` (lines 216-219).
  - It calls `resp.close()` (line 222).
  - It sends the new request with `self.send(req, ..., allow_redirects=False)` (lines 292-301).
  - It calls `get_redirect_target` on the new response to decide whether to loop again, then yields that response (lines 306-307).

**What changes between hops**
1. **URL**
   - A scheme-relative `//host` location gets the scheme of `resp.url` (lines 225-227).
   - If the new URL has no fragment, the previous fragment is carried over (lines 229-235).
   - A relative location is resolved with `urljoin(resp.url, requote_uri(url))`. An absolute one is just requoted (lines 240-245).
2. **Method** (`rebuild_method`, lines 370-392)
   - A 303 becomes GET unless the method is HEAD.
   - A 302 becomes GET unless the method is HEAD.
   - A 301 on a POST becomes GET.
3. **Body and headers** (lines 249-258)
   - For any status other than 307 or 308, `Content-Length`, `Content-Type` and `Transfer-Encoding` are removed and `body` is set to `None`.
   - For 307 and 308 the body is kept. If it is a file-like object it is rewound with `rewind_body` (lines 278-284), and `UnrewindableBodyError` is raised if that fails, per the comment at lines 275-277.
4. **Cookies** (lines 261-269)
   - The `Cookie` header is dropped.
   - Cookies from the response are extracted into the request's jar and merged with the session's jar.
   - `prepare_cookies` then rebuilds the header.
   - After each send, cookies are also extracted into `self.cookies` (line 303).
5. **Auth** (`rebuild_auth`, lines 309-332)
   - `Authorization` is deleted when `should_strip_auth` returns true. That happens if the hostname differs, or if the port or scheme changed.
   - The exception is an http→https upgrade on the standard ports, which keeps the header (lines 154-184).
   - If `trust_env` is on, `.netrc` credentials for the new URL are applied.
6. **Proxies** (`rebuild_proxies`, lines 334-368)
   - The proxy settings are re-resolved for the new URL, which respects `NO_PROXY`.
   - `Proxy-Authorization` is removed and re-added only for non-https schemes when the proxy URL has credentials.

The `stream`, `timeout`, `verify` and `cert` arguments are passed through unchanged.

**Uncertainty:** I did not read lines 782-793 of `send`, which cover dispatching the first request and its hooks. They do not affect the redirect logic.