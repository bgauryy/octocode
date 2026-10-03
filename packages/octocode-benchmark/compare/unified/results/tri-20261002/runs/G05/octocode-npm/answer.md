In `psf/requests` at 611c6162cb, `Session.send` follows redirects by iterating the `resolve_redirects` generator, which sends one new request per hop with `allow_redirects=False`. The code is in `src/requests/sessions.py`. The reads were minified, so I'm citing function names. The only line number I have is `def resolve_redirects` at about line 186.

## How it follows redirects

1. **Entry point.** `Session.request` passes `allow_redirects` (default `True`) to `send`. `send` then runs `gen = self.resolve_redirects(r, request, **kwargs); history = [resp for resp in gen]`. It first extracts cookies from the response and from each response in `r.history`.
2. **History.** After the loop, `send` inserts the original response at position 0 of `history`. It then pops the last response as the final `r` and sets `r.history = history`.
3. **No-follow case.** With `allow_redirects=False`, `send` still calls `resolve_redirects(..., yield_requests=True)` and stores the next prepared request in `r._next`. It catches `StopIteration` when there is no redirect.
4. **Finding the target.** `get_redirect_target` returns `None` unless `resp.is_redirect`. Otherwise it takes the `Location` header, re-encodes it as latin1 and decodes it as UTF-8.
5. **Per hop.** The `while url:` loop in `resolve_redirects` does the following:
   - It copies the request with `req.copy()`.
   - It appends the response to `hist` and sets `resp.history`.
   - It reads `resp.content` to release the socket, falling back to `resp.raw.read(decode_content=False)` on `ChunkedEncodingError`, `ContentDecodingError` or `RuntimeError`.
   - It raises `TooManyRedirects` if `len(resp.history) >= self.max_redirects`.
   - It calls `resp.close()`.
   - It sends the new request via `self.send(..., allow_redirects=False)`, or yields it if `yield_requests` is set.
   - It extracts cookies into `self.cookies` and reads the next target.
   - It yields the response.

## What changes between hops

- **URL.**
  - A `//host` Location gets the scheme of `resp.url`.
  - The previous fragment is carried over if the new URL has none (RFC 7231 §7.1.2).
  - A relative Location is resolved with `urljoin(resp.url, requote_uri(url))`.
  - Otherwise the URL is passed through `requote_uri`.
- **Method (`rebuild_method`).**
  - 303 changes the method to `GET` unless it was `HEAD`.
  - 302 changes the method to `GET` unless it was `HEAD`.
  - 301 changes `POST` to `GET`.
  - 307 and 308 keep the method.
- **Body and headers.** For any status other than 307 or 308, the copy drops `Content-Length`, `Content-Type` and `Transfer-Encoding`, and sets `body = None`. For 307 and 308, the body is kept. If `_body_position` is set and the request has `Content-Length` or `Transfer-Encoding`, it calls `rewind_body`. A body that can't be rewound raises `UnrewindableBodyError`.
- **Cookies.** The `Cookie` header is removed. The jar from the previous request then gets the response's cookies and the session's cookies merged in. `prepare_cookies` rebuilds the header from that jar.
- **Auth (`rebuild_auth`).**
  - `Authorization` is deleted when `should_strip_auth(old_url, new_url)` is true. That is the case when the hostname changes, or when the scheme or port changes. Two cases don't count as a change: an http→https redirect on the default ports (80/None to 443/None), and an unchanged scheme with default ports on both sides.
  - If `trust_env` is set, netrc credentials for the new URL are applied.
- **Proxies (`rebuild_proxies`).** It re-resolves proxies for the new URL via `resolve_proxies`. It removes `Proxy-Authorization`. It re-adds it from the proxy URL's credentials only for a non-https target scheme.
- **Fixed settings.** `stream`, `timeout`, `verify`, `cert` and any adapter kwargs are passed through unchanged.

## Uncertainty
I didn't read the omitted middle of `send`, which includes the hooks and the `Response.history` bookkeeping. I also didn't verify exact line numbers.