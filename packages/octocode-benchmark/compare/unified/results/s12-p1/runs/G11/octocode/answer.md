At commit b5addb64f0, httpx ships five concrete transports, all under `httpx/_transports/`. This is based on the class declarations in that package. I did not read the `Client` or `AsyncClient` code, so the compatibility column comes from each class's base class.

| Transport | Module | Base class | Works with |
|---|---|---|---|
| `HTTPTransport` | `default.py:135` | `BaseTransport` | sync `Client` |
| `AsyncHTTPTransport` | `default.py:279` | `AsyncBaseTransport` | `AsyncClient` |
| `WSGITransport` | `wsgi.py:44` | `BaseTransport` | sync `Client`. Its docstring example uses `httpx.Client(transport=transport)` (`wsgi.py:63`). |
| `ASGITransport` | `asgi.py:63` | `AsyncBaseTransport` | `AsyncClient`. Its docstring example uses `httpx.AsyncClient(transport=transport)` (`asgi.py:73`). |
| `MockTransport` | `mock.py:15` | `AsyncBaseTransport, BaseTransport` | both |

`MockTransport` inherits from both base classes (`mock.py:15`), so it can be used with either client.

The abstract base classes `BaseTransport` (sync) and `AsyncBaseTransport` (async) are defined in `base.py`, at lines 14 and 65. They are for writing custom transports and are not usable transports themselves. `_transports/__init__.py` re-exports all of these names.

**Not verified:**
- I didn't read the method bodies. For `MockTransport`, I'm inferring that it implements both `handle_request` and `handle_async_request` from its two base classes.
- I didn't check whether the transports are re-exported at the top-level `httpx` namespace.