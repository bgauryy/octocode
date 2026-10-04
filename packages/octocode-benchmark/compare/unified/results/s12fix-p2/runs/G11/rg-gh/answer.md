Five concrete transports ship in `httpx/_transports/` at b5addb64f0. I listed that directory's tree and grepped each module's class definitions and handler methods. I did not open `_client.py`, so the check that `Client` and `AsyncClient` accept these types rests on the base-class split, not on the client code.

| Transport | Module | Base class | Works with |
|---|---|---|---|
| `HTTPTransport` | `httpx/_transports/default.py:135` | `BaseTransport`, with `handle_request` at line 230 | Sync `Client` |
| `AsyncHTTPTransport` | `httpx/_transports/default.py:279` | `AsyncBaseTransport`, with `handle_async_request` at line 374 | `AsyncClient` |
| `WSGITransport` | `httpx/_transports/wsgi.py:44` | `BaseTransport`, with `handle_request` at line 91 | Sync `Client` |
| `ASGITransport` | `httpx/_transports/asgi.py:63` | `AsyncBaseTransport`, with `handle_async_request` at line 99 | `AsyncClient` |
| `MockTransport` | `httpx/_transports/mock.py:15` | `AsyncBaseTransport` and `BaseTransport` | Both |

`MockTransport` implements `handle_request` (line 19) and `handle_async_request` (line 29), so it works with both clients.

The two abstract base classes are in `httpx/_transports/base.py`:
- `BaseTransport` at line 14 is the sync interface.
- `AsyncBaseTransport` at line 65 is the async interface.

Supporting stream classes in the same modules are `ResponseStream`, `AsyncResponseStream`, `WSGIByteStream` and `ASGIResponseStream`. They are not transports.