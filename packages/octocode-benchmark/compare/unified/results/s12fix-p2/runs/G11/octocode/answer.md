Five concrete transports ship in `httpx/_transports/` at commit b5addb64f0 (full SHA b5addb64f0161ff6bfe94c124ef76f6a1fba5254). The `Client` and `AsyncClient` compatibility below comes from each class's base class. I didn't read the client code that enforces it.

| Transport | Module | Base class | Works with |
|---|---|---|---|
| `HTTPTransport` | `httpx/_transports/default.py:135` | `BaseTransport` | sync `Client` |
| `AsyncHTTPTransport` | `httpx/_transports/default.py:279` | `AsyncBaseTransport` | `AsyncClient` |
| `ASGITransport` | `httpx/_transports/asgi.py:63` | `AsyncBaseTransport` | `AsyncClient` |
| `WSGITransport` | `httpx/_transports/wsgi.py:44` | `BaseTransport` | sync `Client` |
| `MockTransport` | `httpx/_transports/mock.py:15` | `AsyncBaseTransport, BaseTransport` | both |

- **Abstract bases:** `BaseTransport` (sync) and `AsyncBaseTransport` (async) are defined in `httpx/_transports/base.py:14` and `:65`. They are interfaces for custom transports rather than usable transports themselves.
- **Usage examples in the docstrings:** `wsgi.py:63` shows `httpx.Client(transport=transport)` for `WSGITransport`. `asgi.py:73` shows `httpx.AsyncClient(transport=transport)` for `ASGITransport`. `default.py:20` shows `httpx.Client(transport=transport)` for `HTTPTransport`.
- **Exports:** `httpx/_transports/__init__.py` re-exports all of these through `__all__`.

**Not verified:** I didn't open `mock.py` to check that its sync and async handlers are both implemented. I also didn't check whether `AsyncHTTPTransport` is exported at the top level of `httpx`.