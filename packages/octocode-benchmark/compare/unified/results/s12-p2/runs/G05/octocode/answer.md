A Session follows redirects in `Session.send`, which hands off to the `SessionRedirectMixin.resolve_redirects` generator. All lines below are from `src/requests/sessions.py` at commit 611c6162cbc4ac2020a2f91c7cfa4f3abf9bbb60.

## How redirects are followed

- **Entry point:** `send` pops `allow_redirects`, which defaults to `True` (`:773`). After the first response is sent, hooks run, and cookies are saved, it runs `history = [resp for resp in self.resolve_redirects(r, request, **kwargs)]` (`:802-805`).
- **Result assembly:** If any redirects happened, the original response is inserted at the front of the list. The last response is popped off and returned, with the earlier ones as its `.history` (`:810-815`).
- **Redirects off:** `HEAD` defaults `allow_redirects` to `False` (`:692`). When redirects are off, `send` still calls `resolve_redirects(..., yield_requests=True)` and stores the first yielded request in `r._next` (`:818-822`). That is what `Response.next` returns.
- **The loop:**
  - `get_redirect_target` returns the `Location` header, but only if `resp.is_redirect`. It re-encodes the value from latin1 to UTF-8 (`:134-152`).
  - `while url:` (`:204`) copies the request with `req.copy()`.
  - It appends the response to history, then reads `resp.content` to free the socket.
  - It raises `TooManyRedirects` once `len(resp.history) >= self.max_redirects` (`:216-219`).
  - It closes the response, which releases the connection.
  - Each hop is sent with `self.send(..., allow_redirects=False)`, so the loop never recurses. The next target comes from `get_redirect_target(resp)`, and each response is yielded (`:292-307`).

## What changes between hops

1. **URL (`:224-245`):**
   - A scheme-relative `//host` URL gets the scheme of the previous response URL.
   - A fragment on the old URL is carried over if the new `Location` has none.
   - A relative `Location` is joined to `resp.url` with `urljoin`.
   - The result is percent-encoded with `requote_uri`.
2. **Method (`rebuild_method`, `:370-392`):**
   - A 303 becomes `GET` unless the method is `HEAD`.
   - A 302 becomes `GET` unless the method is `HEAD`.
   - A 301 on a `POST` becomes `GET`.
   - 307 and 308 keep the method.
3. **Body and headers (`:249-258`):** For any status other than 307 or 308, the body is set to `None`. `Content-Length`, `Content-Type` and `Transfer-Encoding` are dropped.
4. **Cookies (`:260-269`):**
   - The `Cookie` header is removed.
   - Cookies from the response are extracted into the request's cookie jar and merged with the session's cookies.
   - The `Cookie` header is then rebuilt with `prepare_cookies`.
   - After each hop, cookies from the new response are also saved into `self.cookies` (`:303`).
5. **Auth (`rebuild_auth`, `:309-332`):**
   - `Authorization` is deleted if `should_strip_auth` says the origin changed. That means a different hostname, or a different scheme or port.
   - An http→https upgrade on the standard ports and a default-port equivalence are exempt (`:154-184`).
   - If `trust_env` is set, `.netrc` credentials for the new URL are applied.
6. **Proxies (`rebuild_proxies`, `:334-368`):**
   - Proxies are re-resolved for the new URL, so `NO_PROXY` is honored.
   - `Proxy-Authorization` is removed and re-added only for non-https schemes when the proxy URL has credentials.
   - The resolved proxies are passed to the next `send`.
7. **Body rewind (`:275-284`):** For 307 and 308, where the body is kept, a file-like body is rewound with `rewind_body`. That only happens if `_body_position` is set and a `Content-Length` or `Transfer-Encoding` header is present.

The `stream`, `timeout`, `verify` and `cert` arguments are passed through unchanged (`:292-301`).

I did not read the `Session.request` or `prepare_request` code in the omitted ranges. The above comes from the `send` and `resolve_redirects` paths.