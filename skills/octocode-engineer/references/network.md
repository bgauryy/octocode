# Network

Load when designing or reviewing HTTP APIs and clients, WebSocket or SSE channels, cookies and sessions, browser security (CORS, CSP), TLS, DNS or connection handling. Transport choice is in [communication](concepts.md#communication); threat modelling, authentication and cryptography in [security](systems-and-delivery.md#security); retries, timeouts and rate limits in [reliability and overload](systems-and-delivery.md#reliability-and-overload).

## Choose quickly

| Need | Start with | Watch |
|---|---|---|
| Request/response API | HTTP with correct method semantics | Idempotency decides what may be retried |
| Server pushes updates to a browser | Server-sent events | One-way; reconnects with `Last-Event-ID`; HTTP/1.1 browsers cap connections per origin |
| Bidirectional, low-latency messages | WebSocket | Authenticate and check `Origin` at the handshake; heartbeats; resume after reconnect |
| Large download or upload that can fail midway | HTTP range requests or chunked resumable uploads | `206 Partial Content`, `Content-Range`; verify the final checksum |
| Browser session | `__Host-` cookie: `Secure; HttpOnly; SameSite=Lax; Path=/` | CSRF protection for state-changing requests still needed |
| Cross-origin browser calls | CORS allowlist on the server | CORS controls who may read a response; the server still authorizes every request |
| Service-to-service | HTTP/2 or gRPC over TLS with pooled connections | Pool size, idle timeouts, deadline propagation |

## HTTP semantics

| Concept | Rule | Watch |
|---|---|---|
| Safe methods | `GET`, `HEAD`, `OPTIONS` change no state | A `GET` with side effects is triggered by prefetchers, crawlers and CSRF |
| Idempotent methods | `PUT`, `DELETE` and the safe methods give the same result when repeated | `POST` and `PATCH` are not: retry them only with an idempotency key |
| Status codes | `4xx` caller fault, `5xx` server fault; `429` and `503` with `Retry-After`; `409` conflict; `412` failed precondition | `200` with an error body breaks clients, caches and monitoring |
| Conditional requests | `ETag` + `If-None-Match` → `304`; `If-Match` for optimistic concurrency → `412` on conflict | Weak ETags (`W/`) are not valid for `If-Match` or range requests |
| Caching | `Cache-Control`: `no-store` (never stored), `no-cache` (store, revalidate each use), `private`, `max-age`, `s-maxage`, `stale-while-revalidate`; `Vary` lists headers that change the response | A shared cache keyed without the cookie or auth header serves one user's response to another: mark personalized responses `private` or `no-store` |
| Redirects | `301`/`302` may turn `POST` into `GET`; `307`/`308` keep method and body | Validate redirect targets (open redirect); most clients drop `Authorization` on cross-origin redirects; check yours |
| Compression | `gzip`, `br`, `zstd` via `Accept-Encoding` | Compressing secrets alongside reflected input leaks them (BREACH); compression can buffer a stream |
| Body and header limits | Cap request size, header size and read time | Slow-read and slow-header clients hold connections open (slowloris) |
| Proxies | `Forwarded` / `X-Forwarded-For` are client-controlled unless a trusted proxy overwrites them | Trust them only from known proxy addresses; never build security decisions or reset links from an unchecked `Host` |
| Request smuggling | Front and back ends disagree on `Content-Length` vs `Transfer-Encoding` | Reject ambiguous framing; keep proxy and server parsers aligned; prefer HTTP/2 end to end |
| Header injection | CR/LF in values split headers | Never copy unvalidated input into headers |

| Version | Gains | Limits |
|---|---|---|
| HTTP/1.1 | Keep-alive, chunked transfer | One request at a time per connection; browsers open a few connections per origin |
| HTTP/2 | Multiplexed streams on one TCP connection, header compression, flow control per stream | A lost TCP packet stalls every stream (transport head-of-line blocking) |
| HTTP/3 | QUIC over UDP: no transport head-of-line blocking, faster handshakes, connection migration | UDP may be blocked or rate-limited; 0-RTT data can be replayed, so accept it only for idempotent requests |

Specs: [HTTP semantics (RFC 9110)](https://www.rfc-editor.org/rfc/rfc9110), [HTTP caching (RFC 9111)](https://www.rfc-editor.org/rfc/rfc9111).

## WebSocket and SSE

| Concern | Rule |
|---|---|
| Handshake security | Browsers send cookies on the upgrade and WebSocket is not covered by CORS: check `Origin` against an allowlist and authenticate at the handshake, or a hostile page can open the socket as the user (cross-site WebSocket hijacking) |
| Tokens | Prefer a cookie or a short-lived single-use ticket over a long-lived token in the URL, which ends up in logs and history |
| Liveness | Ping/pong heartbeats shorter than every proxy and load-balancer idle timeout on the path; close half-open connections |
| Reconnect | Backoff with jitter; resume from a sequence number or `Last-Event-ID` so messages are neither lost nor duplicated |
| Backpressure | Bound each connection's send queue; drop or disconnect slow consumers instead of buffering without limit (browser: watch `bufferedAmount`) |
| Message limits | Cap frame and message size; validate every message as untrusted input |
| Ordering | Holds per connection only; a reconnect starts a new order |
| Scaling | Connections pin to an instance: fan out through pub/sub, drain connections on deploy, and expect a reconnect storm after a restart |
| SSE specifics | `text/event-stream`, automatic reconnect, text only; disable proxy buffering for the route |

Specs: [WebSocket (RFC 6455)](https://www.rfc-editor.org/rfc/rfc6455), [SSE (HTML standard)](https://html.spec.whatwg.org/multipage/server-sent-events.html).

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

References: [OWASP cheat sheets](https://cheatsheetseries.owasp.org/), [MDN HTTP headers](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers).

## TLS, DNS and connections

| Area | Rule | Watch |
|---|---|---|
| Certificate validation | Always verify chain and hostname | A disabled check in a test path ships to production; scan for `verify=False`, `rejectUnauthorized: false`, `InsecureSkipVerify` |
| Certificate lifecycle | Automate renewal; alert on expiry | Pinning a leaf certificate breaks clients at rotation |
| Connection pooling | Reuse connections; size pools from Little's law | Unbounded pools exhaust the server; tiny pools queue requests invisibly |
| Idle timeouts | Client idle timeout shorter than the server's and every proxy's | A reused connection the peer already closed fails on first write (`ECONNRESET`): retry only idempotent requests |
| Timeouts per phase | DNS, connect, TLS, first byte, idle between chunks, total deadline | One total timeout hides which phase stalled; a stream needs an idle timeout, not a total one |
| Nagle and delayed ACK | Small writes can wait tens to hundreds of milliseconds | Set `TCP_NODELAY` for request/response protocols, or batch writes |
| DNS caching | Respect TTLs; some runtimes cache resolutions in-process | Long-lived pools keep old addresses after a failover |
| DNS rebinding and SSRF | Resolve once, check every resolved address against the deny list, connect to that address | Checking the name and then resolving again lets an attacker switch the target |
| Dual stack | Happy Eyeballs tries IPv6 and IPv4 | A broken IPv6 route shows as a connect delay, not an error |
| Ports and `TIME_WAIT` | High connection churn exhausts ephemeral ports | Keep-alive and pooling instead of a connection per request |

## Inspect and prove

| Question | Check |
|---|---|
| Which phase is slow? (timings are cumulative: subtract adjacent values) | `curl -sS -o /dev/null -w 'dns %{time_namelookup} connect %{time_connect} tls %{time_appconnect} ttfb %{time_starttransfer} total %{time_total}\n' "$url"` |
| What did the server send? | `curl -sv "$url"` for status, headers, redirects and the negotiated protocol |
| Is the certificate right? | `openssl s_client -connect "$host:443" -servername "$host" </dev/null` for the chain, expiry and protocol |
| Is a cookie or CORS policy applied? | Browser DevTools network panel: response headers, blocked cookies with the reason, preflight requests |
| Does a cache serve one user's data to another? | Two users request the same URL through the cache; compare bodies and the cache-status headers |

Capture packets (`tcpdump`, Wireshark) only on hosts and traffic you are authorized to inspect.
