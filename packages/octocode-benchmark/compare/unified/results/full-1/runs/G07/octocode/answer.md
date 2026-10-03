**Short answer:** `Starlette.build_middleware_stack()` builds the stack lazily, on the first request. It wraps the router in `ServerErrorMiddleware` (outermost), any user middleware, and `ExceptionMiddleware` (innermost). `ExceptionMiddleware` finds handlers by status code or exception class. Anything it doesn't handle propagates outward to `ServerErrorMiddleware`.

**Assembly** (`starlette/applications.py`, commit 63c5760d8a)
- `__init__` (lines 55-61) stores `exception_handlers`, `user_middleware`, and `middleware_stack = None`. The stack isn't built yet.
- `__call__` (lines 92-96) sets `scope["app"]`, builds the stack if `middleware_stack` is `None`, then calls it.
- `build_middleware_stack` (lines 63-83) does the following:
  - **Split the handlers (lines 68-72).** The handler for key `500` or `Exception` becomes `error_handler`. All others go into `exception_handlers`.
  - **Build the middleware list (lines 74-78).** The order is:
    1. `ServerErrorMiddleware(handler=error_handler, debug=debug)`.
    2. `RequestBodyLimitMiddleware`, but only if `max_body_size` is set.
    3. The user middleware, in the order given.
    4. `ExceptionMiddleware(handlers=exception_handlers, debug=debug)`.
  - **Wrap the router (lines 80-82).** It starts from `app = self.router` and iterates `reversed(middleware)`, wrapping each layer around the previous one. The first list item ends up outermost, with the router at the core.

**Exception routing**
- **Setup, in `ExceptionMiddleware.__call__` (`starlette/middleware/exceptions.py:48-63`):**
  - Non-http/websocket scopes are passed straight through.
  - Otherwise it stores `(self._exception_handlers, self._status_handlers)` in `scope["starlette.exception_handlers"]`.
  - It then builds a `Request` or `WebSocket` and runs `wrap_app_handling_exceptions(self.app, conn)`.
- **Default handlers (lines 22-31):** These are `HTTPException → http_exception` and `WebSocketException → websocket_exception`. User handlers are added through `add_exception_handler`. Integer keys go into the status handlers and classes go into the exception handlers.
- **Catching, in `wrap_app_handling_exceptions` (`starlette/_exception_handler.py:22-63`):**
  - It wraps `send` to track whether `http.response.start` has been sent.
  - It calls the app in a `try` block and catches `Exception`.
  - For an `HTTPException`, it first looks up `status_handlers[exc.status_code]`.
  - If there's no match, `_lookup_exception_handler` walks `type(exc).__mro__` and returns the first class that has a handler. Subclasses therefore fall back to base-class handlers.
  - If there's still no handler, it re-raises the exception.
  - If the response has already started, it raises `RuntimeError("Caught handled exception, but response already started.")`.
  - Otherwise it calls the handler with `(conn, exc)`. Async handlers are awaited and sync ones run in a threadpool. A returned response is then sent through the wrapped `sender`.
- **Unhandled exceptions:** These propagate outward through the user middleware to `ServerErrorMiddleware`. It is the outermost layer and receives the `500`/`Exception` handler, or the debug traceback if `debug` is on. I did not read `ServerErrorMiddleware`'s source, so how it handles the exception is inferred from the constructor arguments and the docstring.
- **Routes and mounts:** `wrap_app_handling_exceptions` reads the handlers from `scope["starlette.exception_handlers"]`. That is why it can also be applied elsewhere, for example around individual routes. I did not open the router code to confirm that.

**Not verified:** the source of `ServerErrorMiddleware` and `RequestBodyLimitMiddleware`, and the router's use of `wrap_app_handling_exceptions`.