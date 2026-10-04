At commit b5addb64f0, httpx ships five concrete transports in `httpx/_transports/`. `HTTPTransport`, `AsyncHTTPTransport`, `ASGITransport`, `WSGITransport` and `MockTransport` are listed in `__init__.py`'s `__all__`, which I only saw in part. I read each class's declaration and base class, not the client code that consumes them. The sync/async split below comes from each class's base class.

| Transport | Module | Base class | Client |
|---|---|---|---|
| `HTTPTransport` | `httpx/_transports/default.py:135` | `BaseTransport` | sync `Client` |
| `AsyncHTTPTransport` | `httpx/_transports/default.py:279` | `AsyncBaseTransport` | `AsyncClient` |
| `ASGITransport` | `httpx/_transports/asgi.py:63` | `AsyncBaseTransport` | `AsyncClient` |
| `WSGITransport` | `httpx/_transports/wsgi.py:44` | `BaseTransport` | sync `Client` |
| `MockTransport` | `httpx/_transports/mock.py:15` | `AsyncBaseTransport` and `BaseTransport` | both |

- **Abstract bases:** `BaseTransport` (sync) and `AsyncBaseTransport` (async) are defined in `httpx/_transports/base.py` at lines 14 and 65. They are interfaces, not usable transports on their own.
- **`ASGITransport` and `WSGITransport`:** their docstrings show the intended pairing. The ASGI one uses `httpx.AsyncClient(transport=transport)` (`asgi.py:73`) and the WSGI one uses `httpx.Client(transport=transport)` (`wsgi.py:63`).
- **`MockTransport`:**
  - It has `handle_request` (`mock.py:19`) for the sync client and `handle_async_request` (`mock.py:29`) for the async client.
  - With the sync `Client`, the handler must be a plain function. An async handler raises `TypeError("Cannot use an async handler in a sync Client")` (`mock.py:26`).
  - With `AsyncClient`, the handler may be either sync or async (`mock.py:36-41`).
- **Helper classes:** `ResponseStream` (`default.py:121`), `AsyncResponseStream` (`default.py:265`), `ASGIResponseStream` (`asgi.py:55`) and `WSGIByteStream` (`wsgi.py:30`) are stream helpers, not transports.