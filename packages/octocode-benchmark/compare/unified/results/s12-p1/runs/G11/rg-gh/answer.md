Five concrete transports ship in `httpx/_transports/` at b5addb64f0. Two are sync-only, two are async-only and one works with both. I read the class definitions and handler methods in each module through the GitHub API. I did not check how `Client` and `AsyncClient` type-check or accept a transport. The sync/async split below comes from which base class each transport extends.

| Transport | Module | Base class | Works with |
|---|---|---|---|
| `HTTPTransport` | `httpx/_transports/default.py:135` | `BaseTransport`, with `handle_request` at line 230 | sync `Client` only |
| `AsyncHTTPTransport` | `httpx/_transports/default.py:279` | `AsyncBaseTransport`, with `handle_async_request` at line 374 | `AsyncClient` only |
| `WSGITransport` | `httpx/_transports/wsgi.py:44` | `BaseTransport`, with `handle_request` at line 91 | sync `Client` only |
| `ASGITransport` | `httpx/_transports/asgi.py:63` | `AsyncBaseTransport`, with `handle_async_request` at line 99 | `AsyncClient` only |
| `MockTransport` | `httpx/_transports/mock.py:15` | `AsyncBaseTransport` and `BaseTransport`, with `handle_request` at line 19 and `handle_async_request` at line 29 | both |

The two abstract base classes are also in `httpx/_transports/base.py`. `BaseTransport` is at line 14, with `handle_request` at line 26. `AsyncBaseTransport` is at line 65, with `handle_async_request` at line 77. They are the interfaces for custom transports rather than usable transports themselves.

The helper stream classes `ResponseStream`, `AsyncResponseStream`, `WSGIByteStream` and `ASGIResponseStream` are internal and not transports.