Load when designing or reviewing HTTP methods, status codes, caching or conditional requests, or a WebSocket or SSE channel.

# Network: HTTP, WebSocket and SSE

## HTTP semantics

| Concept | Rule | Watch |
|---|---|---|
| Safe methods | `GET`, `HEAD`, `OPTIONS` and `TRACE` change no state | A `GET` with side effects is triggered by prefetchers, crawlers and CSRF |
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
