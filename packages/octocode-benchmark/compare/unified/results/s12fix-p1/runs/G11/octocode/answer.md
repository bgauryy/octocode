**Short answer:** the package ships five concrete transports, plus two abstract base classes. Three of the five work with only one client type, and `MockTransport` works with both. All paths and line numbers are at commit b5addb64f0161ff6bfe94c124ef76f6a1fba5254. I got them from a code search and a directory listing. I didn't open the files, so the sync/async mapping comes from each class's base class.

| Transport | Defined in | Base class | Client |
|---|---|---|---|
| `HTTPTransport` | `httpx/_transports/default.py:135` | `BaseTransport` | sync `Client` |
| `AsyncHTTPTransport` | `httpx/_transports/default.py:279` | `AsyncBaseTransport` | `AsyncClient` |
| `WSGITransport` | `httpx/_transports/wsgi.py:44` | `BaseTransport` | sync `Client` |
| `ASGITransport` | `httpx/_transports/asgi.py:63` | `AsyncBaseTransport` | `AsyncClient` |
| `MockTransport` | `httpx/_transports/mock.py:15` | `AsyncBaseTransport, BaseTransport` | both |

- **Abstract bases:** `BaseTransport` (`base.py:14`) is the sync interface, and `AsyncBaseTransport` (`base.py:65`) is the async one.
- **Helper classes:** these are not transports. They are `ResponseStream` and `AsyncResponseStream` in `default.py`, `WSGIByteStream` in `wsgi.py:30`, and `ASGIResponseStream` in `asgi.py:55`.
- **Package layout:** `httpx/_transports/` also contains `__init__.py`.

**Uncertainty:**
- I did not check how `__init__.py` or the top-level `httpx` package re-exports these classes.
- The sync/async labels come from the base classes only. I did not read the `handle_request` and `handle_async_request` bodies.