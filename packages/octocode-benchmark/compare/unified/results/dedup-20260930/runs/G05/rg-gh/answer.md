A Session follows redirects in a loop, `SessionRedirectMixin.resolve_redirects`. It copies the previous request for each hop, changes a few parts of the copy, and re-sends it with `allow_redirects=False`. I read `src/requests/sessions.py` at 611c6162cb, fetched through the GitHub API. The line numbers below are from that file.

**How it follows redirects**
- `Session.send` calls `self.resolve_redirects(r, request, **kwargs)` when `allow_redirects` is true and collects the yielded responses into `history` (`sessions.py:801-805`). It then inserts the original response at the front, makes the last response the result, and sets `r.history = history` (`sessions.py:809-813`).
- `get_redirect_target` returns the `Location` header if `resp.is_redirect`, otherwise `None` (`sessions.py:134-150`). The loop `while url:` runs until there is no redirect target (`sessions.py:190-194`).
- Each hop does the following:
  - It works on `prepared_request = req.copy()` (`sessions.py:195`).
  - It records `resp.history` and appends the response to the history list (`sessions.py:198-199`).
  - It reads `resp.content` to drain the socket, then calls `resp.close()` to release the connection (`sessions.py:201-204`, `sessions.py:212-214`).
  - It raises `TooManyRedirects` once `len(resp.history) >= self.max_redirects` (`sessions.py:216-219`). The default comes from `DEFAULT_REDIRECT_LIMIT` (`sessions.py:488`).
  - It calls `self.send(req, ..., allow_redirects=False)`, stores the response's cookies in `self.cookies`, reads the next target from the response, and yields it (`sessions.py:291-306`).
- With `allow_redirects=False`, `Session.send` still calls `resolve_redirects(..., yield_requests=True)` and stores the first next request in `r._next`. That is what backs `Response.next` (`sessions.py:818-824`). In this mode the loop yields the prepared request and does not send it (`sessions.py:288-289`).

**What changes between hops**
1. **URL** (`sessions.py:216-245`):
   - A scheme-relative `//host` location gets the previous response's scheme.
   - If the new URL has no fragment, the previous fragment is carried over.
   - A relative location is resolved with `urljoin(resp.url, requote_uri(url))`. An absolute one is just requoted.
2. **Method**, in `rebuild_method` (`sessions.py:370-392`):
   - 303 turns any method except HEAD into GET.
   - 302 turns any method except HEAD into GET.
   - 301 turns POST into GET.
   - 307 and 308 keep the method.
3. **Body and headers** (`sessions.py:249-259`):
   - For any status other than 307 or 308, it drops `Content-Length`, `Content-Type` and `Transfer-Encoding`, and sets `body = None`.
   - For 307 and 308 the body is kept. If the body is a file-like object, it is rewound with `rewind_body` (`sessions.py:276-282`).
4. **Cookies** (`sessions.py:261-270`): it removes the `Cookie` header. It then extracts cookies set by the redirect response into the request's cookie jar, merges in `self.cookies`, and re-prepares the `Cookie` header.
5. **Auth**, in `rebuild_auth` (`sessions.py:309-332`):
   - It deletes `Authorization` when `should_strip_auth(old_url, new_url)` is true.
   - `should_strip_auth` returns true when the hostname differs (`sessions.py:154-159`).
   - It also returns true when the port or scheme changes (`sessions.py:179-183`), with two exceptions:
     - http to https on the default ports (`sessions.py:160-171`).
     - A URL that stays on the same scheme and uses the default port or no port (`sessions.py:173-178`).
   - If `trust_env` is set, it looks up `.netrc` credentials for the new URL and applies them.
6. **Proxies**, in `rebuild_proxies` (`sessions.py:334-368`):
   - It re-resolves the proxy settings for the new URL, including `NO_PROXY`.
   - It removes any `Proxy-Authorization` header.
   - It sets a new `Proxy-Authorization` only for non-https schemes when the proxy URL carries credentials.

**Not verified**
- I did not read `resolve_proxies`, `rewind_body` or `is_redirect` (in `models.py`/`utils.py`). Their behavior is described only from how `sessions.py` uses them.
- I did not run any code.