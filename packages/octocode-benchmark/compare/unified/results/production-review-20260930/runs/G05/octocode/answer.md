**I couldn't verify this at commit `611c6162cb`.** No Octocode tools were available in this session, so I have no `path:line` citations. What follows is from my memory of `requests/sessions.py`, and it may differ at that commit.

**How it follows redirects (from memory, unverified)**
- `Session.send` sends the request through the adapter, then, if `allow_redirects` is true, iterates `self.resolve_redirects(r, request, **kwargs)`. It collects the responses into `r.history`.
- If `allow_redirects=False`, it only stores the next request on `r._next`.
- `SessionRedirectMixin.resolve_redirects` loops while `get_redirect_target(resp)` returns a URL.
  - It copies the previous `PreparedRequest` and consumes and closes the old response.
  - It raises `TooManyRedirects` once the history length reaches `max_redirects` (default 30).
  - It resolves the new URL: `//host` inherits the old scheme, and relative URLs go through `urljoin` against `resp.url`. It applies `requote_uri` to the result.
  - It sends the new request with `allow_redirects=False` and repeats.

**What it changes between hops (from memory, unverified)**
- **URL:** set to the resolved `Location`.
- **Method** (`rebuild_method`):
  - 303 becomes GET unless the request was HEAD.
  - 302 becomes GET unless it was HEAD.
  - A 301 POST becomes GET.
  - 307 and 308 keep the method.
- **Body headers:** for any status other than 307 or 308, it drops `Content-Length`, `Content-Type` and `Transfer-Encoding`, and sets `body=None`. For 307/308 it rewinds the body to its recorded position if the body is rewindable.
- **Cookies:** it removes the `Cookie` header, merges cookies from the response and the session jar, and re-prepares the cookie header.
- **Auth** (`rebuild_auth`): it drops `Authorization` if the redirect goes to a different host, with an exception for http→https upgrades on the same host and default ports. If `trust_env` is set, it re-applies netrc credentials for the new host.
- **Proxies** (`rebuild_proxies`): it re-resolves proxies for the new URL. It strips `Proxy-Authorization` and re-adds it if the new proxy URL has credentials.

**Uncertainty:** the status-code handling, the exact auth-stripping conditions and the method names are all from recall. If you'd like this confirmed with line numbers, re-run with the Octocode tools enabled and I'll check `requests/sessions.py` at `611c6162cb`.