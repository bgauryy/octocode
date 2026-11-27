Load when setting cookies or sessions, or browser security headers (CORS, Content-Security-Policy, Strict-Transport-Security).

# Network: cookies, sessions and browser security headers

## Cookies and sessions

| Attribute or practice | Effect | Watch |
|---|---|---|
| `Secure` | Sent only over HTTPS | Required for `SameSite=None` |
| `HttpOnly` | Hidden from page scripts | XSS can still send requests that carry it |
| `SameSite=Lax` / `Strict` / `None` | Controls sending on cross-site requests; Chromium-based browsers treat a missing value as `Lax`; Firefox and Safari do not | `Lax` still sends on top-level `GET` navigations; same-site subdomains count as same site |
| `Domain` | Omit it for a host-only cookie | Setting it shares the cookie with every subdomain |
| `Path` | Scopes sending | Not a security boundary |
| `__Host-` prefix | Requires `Secure`, `Path=/`, no `Domain` | Strongest default for session cookies |
| `Max-Age` / `Expires` | Persistent cookie; omitted means a session cookie | Long-lived session cookies widen theft windows |
| Size | About 4 KB per cookie, sent on every matching request | Keep state server-side; store an id, not data |
| Session fixation | Issue a new session id at login and privilege change | Invalidate it server-side on logout |
| CSRF | Anti-CSRF token or a checked `Origin` / `Sec-Fetch-Site` on every state-changing request | `SameSite` alone is defense in depth, not the whole fix |
| Cookie vs token storage | `localStorage` is readable by any injected script; an `HttpOnly` cookie is not | Neither survives XSS: fix XSS first |

Spec: [HTTP state management (RFC 6265)](https://www.rfc-editor.org/rfc/rfc6265) and its in-progress revision (rfc6265bis) for `SameSite` and prefixes.

## Browser security headers

| Header or policy | Purpose | Watch |
|---|---|---|
| CORS (`Access-Control-Allow-*`) | Lets named origins read responses | `*` cannot be used with credentials; reflecting any `Origin` with credentials lets every site read user data; send `Vary: Origin` for dynamic answers; treat the `null` origin as untrusted |
| Content-Security-Policy | Limits script, frame and connection sources | Use nonces or hashes for scripts; `frame-ancestors` replaces `X-Frame-Options` against clickjacking; roll out with `Content-Security-Policy-Report-Only` |
| Strict-Transport-Security | Forces HTTPS for later visits | `includeSubDomains` and preload are hard to undo |
| `X-Content-Type-Options: nosniff` | Stops content-type sniffing | Send a correct `Content-Type` with charset |
| Referrer-Policy | Limits URL leakage to other sites | Secrets in URLs leak through referrers and logs anyway |
| COOP / COEP | Cross-origin isolation | Required for `SharedArrayBuffer` and high-resolution timers |
| Permissions-Policy | Disables powerful features (camera, geolocation) per origin | Third-party frames inherit only what is granted |

References: [OWASP cheat sheets](https://cheatsheetseries.owasp.org/), [MDN HTTP headers](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers).
