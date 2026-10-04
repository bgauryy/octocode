Five concrete transports ship with httpx at b5addb64f0. They live in `httpx/_transports/`. Two work with both clients, two with only one client each, and `MockTransport` works with both. I read the files through the GitHub API, so I haven't run any of this.

| Transport | Module | Base class | Usable with |
|---|---|---|---|
| `HTTPTransport` | `httpx/_transports/default.py:135` | `BaseTransport` | sync `Client` only (`handle_request`, line 230) |
| `AsyncHTTPTransport` | `httpx/_transports/default.py:279` | `AsyncBaseTransport` | `AsyncClient` only (`handle_async_request`, line 374) |
| `WSGITransport` | `httpx/_transports/wsgi.py:44` | `BaseTransport` | sync `Client` only (`handle_request`, line 91) |
| `ASGITransport` | `httpx/_transports/asgi.py:63` | `AsyncBaseTransport` | `AsyncClient` only (`handle_async_request`, line 99) |
| `MockTransport` | `httpx/_transports/mock.py:15` | `AsyncBaseTransport` and `BaseTransport` | both (`handle_request`, line 19, and `handle_async_request`, line 29) |

The two abstract base classes are in `httpx/_transports/base.py`: `BaseTransport` at line 14 and `AsyncBaseTransport` at line 65. They define the interfaces but aren't usable transports themselves.

My sync/async split comes from which base class each transport extends and which handler method it defines. I didn't read how `Client` and `AsyncClient` check the transport type they're given.