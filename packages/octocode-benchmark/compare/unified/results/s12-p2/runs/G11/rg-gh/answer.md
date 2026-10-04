httpx at b5addb64f0 ships five concrete transports, all in `httpx/_transports/`. Two abstract base classes sit alongside them. I read the class definitions and their handler methods. I did not check how `Client` and `AsyncClient` type-check or validate the `transport=` argument.

| Transport | Defined in | Handler method | Works with |
|---|---|---|---|
| `HTTPTransport` | `default.py:135` | `handle_request` (`default.py:230`) | `Client` |
| `AsyncHTTPTransport` | `default.py:279` | `handle_async_request` (`default.py:374`) | `AsyncClient` |
| `WSGITransport` | `wsgi.py:44` | `handle_request` (`wsgi.py:91`) | `Client` |
| `ASGITransport` | `asgi.py:63` | `handle_async_request` (`asgi.py:99`) | `AsyncClient` |
| `MockTransport` | `mock.py:15` | both `handle_request` (`mock.py:19`) and `handle_async_request` (`mock.py:29`) | `Client` and `AsyncClient` |

- **Base classes:** both are in `base.py`. `BaseTransport` (`base.py:14`) is the sync interface with `handle_request`. `AsyncBaseTransport` (`base.py:65`) is the async interface with `handle_async_request`.
- **Which client a transport works with:** I inferred this from the base class each transport subclasses. `HTTPTransport` and `WSGITransport` subclass `BaseTransport`. `AsyncHTTPTransport` and `ASGITransport` subclass `AsyncBaseTransport`. `MockTransport` subclasses both (`mock.py:15`), so it works with either client.
- **Mock handler:** `MockTransport.__init__` takes a handler typed `SyncHandler | AsyncHandler` (`mock.py:16`). I did not check which handler type each client path requires.