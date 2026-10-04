A Session follows redirects by looping in a generator, `SessionRedirectMixin.resolve_redirects`. `Session.send` drives it, and each hop is sent with `allow_redirects=False`. All lines below are in `src/requests/sessions.py` at commit 611c6162cbc4.

**How the loop runs**
- `Session.send` pops `allow_redirects` (default True, `:773`). If it is true, it calls `gen = self.resolve_redirects(r, request, **kwargs)` and collects the results as `history` (`:802-805`).
- When redirects are off, it still calls `resolve_redirects(..., yield_requests=True)` once and stores the next prepared request in `r._next`, which `Response.next` returns (`:818-824`).
- `get_redirect_target` returns the `Location` header if `resp.is_redirect`, otherwise `None` (`:134-152`). It re-encodes the header from latin1 to utf8 first.
- `resolve_redirects` loops `while url:` (`:204`). Each pass:
  - copies the previous request with `req.copy()` (`:205`);
  - sets `resp.history` and appends `resp` to the history list (`:208-209`);
  - reads the response body so the socket is released (`:212-214`);
  - raises `TooManyRedirects` if `len(resp.history) >= self.max_redirects` (`:216-219`);
  - calls `resp.close()` (`:222`);
  - sends the new request through `self.send(..., allow_redirects=False)` (`:292-301`);
  - stores the cookies from the response in `self.cookies` (`:303`);
  - reads the next redirect target and yields the response (`:306-307`).

**What changes between hops**
1. **URL (`:225-245`)**
   - A scheme-relative `//host` location gets the scheme of the previous response URL.
   - If the new URL has no fragment, the previous fragment is carried over.
   - A relative location is joined to `resp.url` with `urljoin`.
   - The URL is requoted with `requote_uri`.
2. **Method, via `rebuild_method` (`:370-392`)**
   - A 303 becomes GET unless the method is HEAD.
   - A 302 becomes GET unless the method is HEAD.
   - A 301 turns POST into GET.
3. **Body and headers for status codes other than 307 and 308 (`:250-258`)**
   - `Content-Length`, `Content-Type` and `Transfer-Encoding` are removed.
   - The body is set to `None`.
   - 307 and 308 keep the body and those headers.
   - If the body is a file-like object that was already read, it is rewound with `rewind_body` when `Content-Length` or `Transfer-Encoding` is present (`:278-284`).
4. **Cookies (`:260-269`)**
   - The old `Cookie` header is dropped.
   - Cookies from the response are extracted into the request's cookie jar and merged with the session's cookies.
   - The `Cookie` header is rebuilt with `prepare_cookies`.
5. **Auth, via `rebuild_auth` (`:309-332`)**
   - The `Authorization` header is deleted if `should_strip_auth(original_url, url)` is true. It returns true when the hostname changes.
   - It also returns true when the scheme or port changes, with two exceptions. An http to https move on the default ports (80 or none, to 443 or none) keeps auth. A same-scheme change between default-port forms keeps it too (`:154-184`).
   - If `trust_env` is on, `.netrc` credentials for the new URL are applied.
6. **Proxies, via `rebuild_proxies` (`:334-368`)**
   - Proxies are re-resolved for the new URL, so `NO_PROXY` is honoured.
   - The `Proxy-Authorization` header is removed.
   - It is set again from the proxy URL's credentials only when the scheme is not https.

The `stream`, `timeout`, `verify`, `cert` and `proxies` arguments are passed unchanged to each hop (`:292-301`). `proxies` is the exception, since the re-resolved value from step 6 is what gets passed on.

I read this code but did not run it.