A `Session` follows redirects through `SessionRedirectMixin.resolve_redirects`, a generator that `Session.send` drives. All lines below are from `src/requests/sessions.py` at 611c6162cbc4ac2020a2f91c7cfa4f3abf9bbb60.

## How it follows redirects

- **Entry point:** `Session.send` sends the first request. If `allow_redirects` is true (the default, popped at line 773), it calls `gen = self.resolve_redirects(r, request, **kwargs)` and collects every yielded response into `history` (lines 802-805). The last response becomes the result, and the earlier ones go into `r.history` (lines 812-815).
- **No-follow case:** If `allow_redirects` is false, `send` calls `resolve_redirects(..., yield_requests=True)` once and stores the next prepared request on `r._next` (lines 818-824). `Response.next` uses that.
- **Loop condition:** `get_redirect_target` returns the `Location` header only if `resp.is_redirect`. It re-encodes the value from latin1 to UTF-8 (lines 134-152). The loop runs `while url:` (line 204).
- **Each hop:**
  1. It copies the previous request with `req.copy()` (line 205).
  2. It appends the response to history, reads `resp.content` to drain the socket, and closes the response (lines 208-222).
  3. It raises `TooManyRedirects` once `len(resp.history) >= self.max_redirects` (lines 216-219).
  4. It sends the new request with `self.send(req, ..., allow_redirects=False)` (lines 292-301). Redirects are therefore handled by this loop and not by nested `send` calls.
  5. It extracts cookies into the session jar and reads the next redirect target (lines 303-306).

## What changes between hops

- **URL (lines 224-245):**
  - A scheme-relative `//host` location gets the scheme of the previous response URL.
  - A fragment-less location inherits the previous fragment.
  - A relative location is joined with `resp.url` via `urljoin`.
  - The result is run through `requote_uri`.
- **Method (`rebuild_method`, lines 370-392):**
  - 303 changes any method except HEAD to GET.
  - 302 changes any method except HEAD to GET.
  - 301 changes POST to GET.
  - 307 and 308 keep the method.
- **Body and headers (lines 249-258):** For any redirect status other than 307 or 308, it removes `Content-Length`, `Content-Type` and `Transfer-Encoding` and sets `body = None`.
- **Cookies (lines 260-269):**
  - It drops the old `Cookie` header.
  - It extracts cookies from the redirect response into the request's cookie jar and merges in the session's cookies.
  - It then calls `prepare_cookies` to rebuild the header.
- **Auth (`rebuild_auth`, lines 309-332):**
  - It deletes `Authorization` when `should_strip_auth` says so. That is true if the hostname differs, or if the port or scheme changes.
  - Two cases are exempt: an http→https upgrade on the default ports, and the same scheme with default ports on both sides (lines 154-184).
  - If `trust_env` is set, it re-applies `.netrc` credentials for the new URL.
- **Proxies (`rebuild_proxies`, lines 334-368):**
  - It re-resolves the proxy configuration for the new URL, which includes honouring `NO_PROXY`.
  - It removes any `Proxy-Authorization` header.
  - It re-adds `Proxy-Authorization` only for non-https schemes when the proxy URL has credentials.
- **Body rewind (lines 275-284):** If the body was a file-like object with a recorded position and a length or transfer-encoding header is still present, it calls `rewind_body`. This matters for 307 and 308, where the body is kept.

The `resolve_redirects` source is lines 186-307, and I read it directly. I did not read lines 780-795 or 809-811 of `send`, which are the hooks dispatch and a small history step. They don't change the answer.