Load when designing or reviewing HTTP APIs and clients, WebSocket or SSE channels, cookies and sessions, browser security (CORS, CSP), TLS, DNS or connection handling. Transport choice is in [communication](concepts-communication.md#communication); threat modelling, authentication and cryptography in [security](systems-security.md#security); retries, timeouts and rate limits in [reliability and overload](systems-reliability.md#reliability-and-overload).

Read next: [HTTP, WebSocket and SSE](network-http.md) for methods, status codes, caching and long-lived channels; [cookies, sessions and browser security headers](network-browser-security.md) for cookies, CORS and CSP.

# Network

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
