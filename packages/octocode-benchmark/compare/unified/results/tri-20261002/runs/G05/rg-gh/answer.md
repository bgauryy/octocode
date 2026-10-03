A `Session` follows redirects in `Session.send`, which hands the first response to the generator `SessionRedirectMixin.resolve_redirects`. That generator copies the previous request, edits the copy, and sends it again. All paths and line numbers below are from `src/requests/sessions.py` at 611c6162cb.

**How the loop works**
- `Session.send` sends the request. If `allow_redirects` is true, it runs `gen = self.resolve_redirects(r, request, **kwargs)` and collects every response into `history` (lines 802-805).
- It then puts the first response at the front of `history` and pops the last one as the final response `r`. The rest becomes `r.history` (lines 810-815).
- `get_redirect_target` returns the `Location` header, re-encoded from latin1 to UTF-8, but only when `resp.is_redirect` is true (lines 134-152). `resolve_redirects` loops `while url:` (line 204).
- Each iteration does the following:
  - It copies the previous request with `req.copy()` (line 205).
  - It appends the response to `history` and reads `resp.content` so the connection can be released (lines 208-214).
  - It raises `TooManyRedirects` when `len(resp.history) >= self.max_redirects` (lines 216-219).
  - It closes the response (line 222).
  - It sends the new request with `allow_redirects=False`, so the loop itself handles each hop (lines 292-301).
  - It stores any cookies from the new response in `self.cookies` (line 303).
  - It reads the next redirect target and yields the response (lines 306-307).
- When redirects are not followed, `send` calls `resolve_redirects(..., yield_requests=True)` once. That yields the next `PreparedRequest` instead of sending it, and `send` stores it in `r._next` (lines 817-824).

**What changes between hops**
1. **URL (lines 224-245).**
   - A `//host` location gets the scheme of the previous response URL.
   - A fragment is carried over from the previous URL if the new location has none (lines 229-235).
   - A relative location is joined to `resp.url` with `urljoin`, and the result is percent-encoded with `requote_uri`.
2. **Method (`rebuild_method`, lines 370-392).**
   - 303 becomes GET unless the method is HEAD.
   - 302 becomes GET unless the method is HEAD.
   - 301 turns POST into GET.
3. **Body and entity headers (lines 249-258).**
   - For any status other than 307 or 308, `Content-Length`, `Content-Type` and `Transfer-Encoding` are removed, and `body` is set to `None`.
   - For 307 and 308 the body is kept. If it is a file-like object, it is rewound with `rewind_body` when a body position was recorded and a length or encoding header is present (lines 275-284).
4. **Cookies (lines 260-269).**
   - The old `Cookie` header is dropped.
   - Cookies from the response are extracted into the request's jar and merged with the session's cookies. The `Cookie` header is then rebuilt with `prepare_cookies`.
5. **Auth (`rebuild_auth`, lines 309-332).**
   - `Authorization` is deleted when `should_strip_auth` says the origin changed (lines 154-184):
     - Any hostname change strips it.
     - An http→https upgrade on the default ports (80 to 443) keeps it.
     - Otherwise a port or scheme change strips it.
   - If `trust_env` is set, netrc credentials for the new URL are applied (lines 330-332).
6. **Proxies (`rebuild_proxies`, lines 334-368).**
   - Proxies are re-resolved for the new URL, which can account for `NO_PROXY`.
   - `Proxy-Authorization` is removed. It is set again from the proxy URL's credentials only when the scheme is not https.

**Uncertainty:** I read only `sessions.py`. I did not check the behavior of `is_redirect`, `rewind_body` or `resolve_proxies` in their own modules.