A Session follows redirects through a generator, `SessionRedirectMixin.resolve_redirects`. `Session.send` drives that generator, and each hop is sent as a fresh copy of the previous request. All line numbers below are from `src/requests/sessions.py` at 611c6162cb.

## How it follows redirects

- **Entry point:** `Session.send` pops `allow_redirects`, which defaults to True (`:773`). It sends the request and, if `allow_redirects` is set, runs `history = [resp for resp in self.resolve_redirects(r, request, **kwargs)]` (`:802-805`).
- **History:** the original response is inserted at the front of the list and the last response is popped as the result. That result gets `r.history = history` (`:810-815`).
- **Redirects not followed:** with `allow_redirects=False`, `send` calls `resolve_redirects(..., yield_requests=True)` once. It stores the first prepared request in `r._next`, which backs `Response.next` (`:817-824`).
- **Detecting a redirect:** `get_redirect_target` returns the `Location` header only if `resp.is_redirect`. It re-encodes the header from latin1 to UTF-8 (`:134-152`).
- **The loop** (`:204-307`):
  1. Copy the previous request with `req.copy()` (`:205`).
  2. Append the response to `hist` and consume its body (`:208-214`).
  3. Raise `TooManyRedirects` once `len(resp.history) >= self.max_redirects` (`:216-219`).
  4. Close the response to release the connection (`:222`).
  5. Send the new request with `allow_redirects=False` (`:292-301`), so the loop controls every hop.
  6. Extract cookies from the response into `self.cookies` (`:303`).
  7. Read the next redirect target and yield the response (`:306-307`).

## What changes between hops

- **URL** (`:224-245`):
  - A `//host` location gets the scheme of the previous response URL.
  - A fragment-less location inherits the previous fragment (RFC 7231 §7.1.2).
  - A relative location is joined to `resp.url` with `urljoin`.
  - The result is percent-encoded with `requote_uri`.
- **Method** (`rebuild_method`, `:370-392`):
  - 303 becomes GET, unless the method is HEAD.
  - 302 becomes GET, unless the method is HEAD.
  - A POST answered with 301 becomes GET.
  - 307 and 308 keep the method.
- **Body and headers** (`:250-258`): for any status other than 307 or 308, the loop removes `Content-Length`, `Content-Type` and `Transfer-Encoding` and sets `body = None`.
- **Cookies** (`:260-269`):
  - The `Cookie` header is dropped.
  - The jar is rebuilt from the response's cookies plus `self.cookies`.
  - The `Cookie` header is then re-prepared from that jar.
- **Auth** (`rebuild_auth`, `:309-332`):
  - `Authorization` is deleted if `should_strip_auth` is true. That is the case when the hostname differs, or when the scheme or port changes (`:154-184`).
  - As an exception, http→https on the default ports keeps the header.
  - If `trust_env` is set, `.netrc` credentials for the new URL are applied.
- **Proxies** (`rebuild_proxies`, `:334-368`):
  - Proxies are re-resolved for the new URL, so `NO_PROXY` is honored.
  - `Proxy-Authorization` is removed.
  - It is re-added only for a non-https scheme when the proxy URL has credentials.
- **Body rewind** (`:275-284`): if the body is a file-like object with `Content-Length` or `Transfer-Encoding` still set (that is, a 307 or 308), `rewind_body` resets it so it can be resent.

## Uncertainty

I read only `sessions.py`. I did not open `utils.py` for `rewind_body` or `resolve_proxies`, or `models.py` for `is_redirect`. Their behavior is inferred from the names and call sites.