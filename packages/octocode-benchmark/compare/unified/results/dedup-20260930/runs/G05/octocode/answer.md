A `Session` follows redirects by sending the request with `allow_redirects=False` on each hop. `SessionRedirectMixin.resolve_redirects` loops over the hops and yields each response. Everything below is from `src/requests/sessions.py` at 611c6162cb. I only have exact line numbers for `resolve_redirects` (starts at line 186) and the "Override the original request" comment (line 286). The other lines are approximate, counted from those two.

I did not read `Session.send`, so I did not verify how it calls `resolve_redirects` or how it builds `resp.history` for the caller.

**The loop (`resolve_redirects`, line 186 onward)**
- `get_redirect_target` returns the `Location` header when `resp.is_redirect`. It re-encodes the value from latin1 to UTF-8 and returns `None` otherwise.
- While there is a target URL, each iteration copies the last request with `req.copy()`.
- It appends the current response to `history` and reads `resp.content` to drain the socket. If that raises, it falls back to `resp.raw.read(decode_content=False)`.
- If the history already has `max_redirects` entries or more, it raises `TooManyRedirects`.
- It closes the response, builds the next request, and calls `self.send(req, ..., allow_redirects=False)`.
- After each `send`, it stores the response cookies in `self.cookies` and reads the next redirect target. It yields the response.
- With `yield_requests=True` it yields the prepared request instead of sending it (internal use).

**What changes between hops**
- **URL:**
  - A `//host` location gets the scheme of the previous response.
  - The fragment is normalised, and the previous fragment is carried over if the new URL has none.
  - Relative locations are joined to `resp.url` with `urljoin`. All locations go through `requote_uri`.
- **Method (`rebuild_method`):**
  - 303 becomes GET unless the method is HEAD.
  - 302 becomes GET unless the method is HEAD.
  - 301 becomes GET only if the method was POST.
  - 307 and 308 keep the method.
- **Body and headers:**
  - For any status other than 307 or 308, it removes `Content-Length`, `Content-Type` and `Transfer-Encoding`, and sets `body = None`.
  - For 307 and 308 the body is kept. If the body is file-like and rewindable, `rewind_body` resets it.
- **Cookies:**
  - It drops the `Cookie` header.
  - It pulls cookies from the response into the request's jar, merges in `self.cookies`, and re-runs `prepare_cookies`.
- **Auth (`rebuild_auth`):**
  - It deletes `Authorization` if `should_strip_auth` says so. That is true when the hostname differs, or the port or scheme changed. The exceptions are an http→https upgrade on the default ports and unchanged default ports.
  - If `trust_env` is set, it re-applies `.netrc` credentials for the new URL.
- **Proxies (`rebuild_proxies`):**
  - It re-resolves the proxy settings for the new URL, which respects `NO_PROXY`.
  - It always removes `Proxy-Authorization`. It sets it again from credentials in the proxy URL, but only for non-https schemes.

**Uncertainty:** Line numbers other than 186 and 286 are estimates.