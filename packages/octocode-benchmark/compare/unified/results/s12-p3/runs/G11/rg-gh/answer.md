At commit b5addb64f0, httpx ships five concrete transports, all under `httpx/_transports/`. Two of them work with both clients, two only with the sync `Client`, and one only with `AsyncClient`. I read the class definitions through the GitHub API. I did not run any code, and I did not open `Client` or `AsyncClient` to check how they validate the transport they're given. The sync/async split comes from which base class each transport extends and which handler method it defines.

| Transport | Module | Base class | Handler method | Usable with |
|---|---|---|---|---|
| `HTTPTransport` | `default.py:135` | `BaseTransport` | `handle_request` (`default.py:230`) | `Client` only |
| `AsyncHTTPTransport` | `default.py:279` | `AsyncBaseTransport` | `handle_async_request` (`default.py:374`) | `AsyncClient` only |
| `WSGITransport` | `wsgi.py:44` | `BaseTransport` | `handle_request` (`wsgi.py:91`) | `Client` only |
| `ASGITransport` | `asgi.py:63` | `AsyncBaseTransport` | `handle_async_request` (`asgi.py:99`) | `AsyncClient` only |
| `MockTransport` | `mock.py:15` | `AsyncBaseTransport` and `BaseTransport` | `handle_request` (`mock.py:19`) and `handle_async_request` (`mock.py:29`) | Both |

- **Base classes:** `BaseTransport` (`base.py:14`) and `AsyncBaseTransport` (`base.py:65`) are the abstract interfaces, not usable transports.
- **Helper classes:** these are streaming helpers, not transports. They are `ResponseStream` (`default.py:121`), `AsyncResponseStream` (`default.py:265`), `WSGIByteStream` (`wsgi.py:30`) and `ASGIResponseStream` (`asgi.py:55`).
- **Both-client case:** `MockTransport` is the only transport that implements both handler methods, so it is the only one that works with either client.
- **Not checked:** `httpx/_transports/__init__.py` exists, but I didn't read it, so I can't say which of these names it re-exports.