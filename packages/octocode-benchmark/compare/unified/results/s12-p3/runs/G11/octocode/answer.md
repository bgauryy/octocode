Five concrete transports ship with httpx at b5addb64f0 (full SHA b5addb64f0161ff6bfe94c124ef76f6a1fba5254). `MockTransport` works with both clients. `HTTPTransport` and `WSGITransport` work only with the sync `Client`. `AsyncHTTPTransport` and `ASGITransport` work only with `AsyncClient`. I judged client compatibility from the base class each transport subclasses. I did not read the `Client` or `AsyncClient` code that checks the transport type.

All of them live in the `httpx/_transports/` package:

| Transport | Module | Base class | Client |
|---|---|---|---|
| `HTTPTransport` | `httpx/_transports/default.py:135` | `BaseTransport` | sync `Client` only |
| `AsyncHTTPTransport` | `httpx/_transports/default.py:279` | `AsyncBaseTransport` | `AsyncClient` only |
| `WSGITransport` | `httpx/_transports/wsgi.py:44` | `BaseTransport` | sync `Client` only |
| `ASGITransport` | `httpx/_transports/asgi.py` (class line not captured) | `AsyncBaseTransport` | `AsyncClient` only |
| `MockTransport` | `httpx/_transports/mock.py:15` | `AsyncBaseTransport, BaseTransport` | both |

- **Abstract bases:** `BaseTransport` (`base.py:14`) is the sync interface and `AsyncBaseTransport` (`base.py:65`) is the async one. These are interfaces, not usable transports on their own.
- **`MockTransport`:** it inherits from both bases, which is why it fits either client.

I only looked at class definitions and base classes, so I haven't checked how `__init__.py` re-exports these.