A `Session` follows redirects through `SessionRedirectMixin.resolve_redirects` in `src/requests/sessions.py`, at commit 611c6162cb. It is a generator that copies the previous request, edits the copy, and sends it again with `allow_redirects=False`. Line numbers below are approximate, counted from the fetched ranges. I only saw `def resolve_redirects` at line 186 directly.

**How it loops**
- `get_redirect_target` returns the `Location` header only if `resp.is_redirect`. It re-encodes the header from latin1 to UTF-8.
- While a target URL exists, each iteration:
  - copies the request with `req.copy()`;
  - appends the response to `resp.history`;
  - reads `resp.content` to drain the socket, falling back to `resp.raw.read(decode_content=False)` on decoding errors;
  - raises `TooManyRedirects` once `len(resp.history) >= self.max_redirects`;
  - calls `resp.close()` to release the connection.
- After editing the copy (below), it becomes the new `req`. If `yield_requests` is set, the loop yields the request. Otherwise it calls `self.send(req, ..., allow_redirects=False, **adapter_kwargs)`, extracts cookies into `self.cookies`, computes the next target and yields the response. That send call is at about lines 292–300.

**What it changes on the request between hops**
1. **URL** (about lines 220–250):
   - A scheme-relative `//host` URL takes the scheme of `resp.url`.
   - The previous fragment is carried over if the new URL has none.
   - A relative `Location` is resolved with `urljoin(resp.url, requote_uri(url))`. An absolute one is requoted.
2. **Method** (`rebuild_method`, at the end of the mixin):
   - 303 becomes GET unless the method is HEAD.
   - 302 becomes GET unless the method is HEAD.
   - 301 becomes GET only if the method was POST.
   - 307 and 308 keep the method.
3. **Body and headers**:
   - For any status other than 307 or 308, it drops `Content-Length`, `Content-Type` and `Transfer-Encoding` and sets `body = None` (issue 3490).
   - For 307 and 308, the body stays. If the body is a rewindable file-like object, `rewind_body` rewinds it. A failed `tell()` raises `UnrewindableBodyError`.
4. **Cookies**:
   - It pops the `Cookie` header.
   - It extracts cookies from the response into the request's cookie jar and merges in `self.cookies`.
   - It re-prepares the `Cookie` header with `prepare_cookies`.
5. **Auth** (`rebuild_auth`):
   - It deletes `Authorization` when `should_strip_auth(old_url, new_url)` is true. That is the case when the hostname changes, or the port or scheme changes.
   - Exceptions: an http→https upgrade on the standard ports, and default-port equivalence, keep it.
   - If `trust_env` is on, it reapplies `.netrc` credentials for the new URL.
6. **Proxies** (`rebuild_proxies`):
   - It re-resolves proxies for the new URL through `resolve_proxies`, so `NO_PROXY` is honoured.
   - It always deletes `Proxy-Authorization`. For non-https schemes with proxy credentials it re-adds it as a Basic header.
   - The resolved proxies are passed to the next `send`.

**Not verified:** I did not read the code where `Session.send` calls `resolve_redirects`, or where `allow_redirects` is handled. I also did not read `Session.request`.