The stack is built lazily on the first request by `Starlette.build_middleware_stack`. Exceptions are caught by `wrap_app_handling_exceptions`, which `ExceptionMiddleware` and the routing layer both use. All line numbers are at 63c5760d8a unless noted.

## Stack assembly

- **Trigger:** `Starlette.__call__` sets `scope["app"]` and, if `self.middleware_stack is None`, calls `build_middleware_stack()`. It then awaits the result (`starlette/applications.py:92-96`).
- **Handler split:** `build_middleware_stack` loops over `self.exception_handlers` (`applications.py:68-72`).
  - Handlers keyed `500` or `Exception` become `error_handler`, which goes to `ServerErrorMiddleware`.
  - Every other handler goes into the `exception_handlers` dict, which goes to `ExceptionMiddleware`.
- **Order:** the `middleware` list is built outermost first (`applications.py:74-78`).
  1. `ServerErrorMiddleware(handler=error_handler, debug=debug)`.
  2. `RequestBodyLimitMiddleware`, only if `max_body_size` is set.
  3. `self.user_middleware`.
  4. `ExceptionMiddleware(handlers=exception_handlers, debug=debug)`.
- **Wrapping:** the list is wrapped around `self.router` in reverse (`applications.py:80-83`). The first entry ends up outermost, and `Router` is the innermost app.
- **User middleware order:** `add_middleware` does `user_middleware.insert(0, ...)` (`applications.py:107`). The most recently added middleware is therefore outermost among the user middleware. It raises `RuntimeError` if the stack is already built, but the check is marked `# pragma: no cover` (`applications.py:105-106`).

## How exceptions reach handlers

1. **`ExceptionMiddleware`** (`starlette/middleware/exceptions.py`):
   - Its `__init__` registers default handlers for `HTTPException` and `WebSocketException` (lines 28-31).
   - It then adds the user's handlers through `add_exception_handler` (lines 32-34). Integer keys go to `_status_handlers`, and exception classes go to `_exception_handlers` (lines 41-45).
   - On `http` and `websocket` scopes, `__call__` stores both dicts in `scope["starlette.exception_handlers"]` (lines 52-55). It builds a `Request` or `WebSocket` as `conn`, then runs `wrap_app_handling_exceptions(self.app, conn)` (lines 57-63). Other scope types, such as lifespan, skip this and go straight to the inner app (lines 48-50).
2. **`wrap_app_handling_exceptions`** (`starlette/_exception_handler.py:23-65`):
   - It reads the two handler dicts from `conn.scope`, and falls back to empty dicts if the key is missing (lines 26-29).
   - It wraps `send` to record whether `http.response.start` has been sent (lines 34-39).
   - It runs `app` in a `try` and catches `Exception` (lines 41-43). Handler lookup then goes in this order:
     - For an `HTTPException`, it first looks up `status_handlers[exc.status_code]` (lines 46-47).
     - If that finds nothing, it calls `_lookup_exception_handler`, which walks `type(exc).__mro__` and takes the first class that has a handler (lines 16-20, 49-50). Subclass handlers therefore win over base-class ones.
     - If there is still no handler, it re-raises the exception (lines 52-53).
   - If the response has already started, it raises `RuntimeError("Caught handled exception, but response already started.")` (lines 55-56).
   - Otherwise it calls the handler with `(conn, exc)`. Async handlers are awaited, and sync handlers run through `run_in_threadpool` (lines 58-61). If the handler returns a response, it is sent through `sender` (lines 62-63).
3. **Routing layer:** `starlette/routing.py` also calls `wrap_app_handling_exceptions` for each request, at lines 65 and 84. These are the HTTP request handler and the websocket session handler. I did not read those lines, only saw them in search results. The search ran against the default branch (4e7fc04), not 63c5760d8a, so those line numbers may differ at the pinned commit. Exceptions raised in an endpoint can therefore be handled at the route level, before they reach `ExceptionMiddleware`. This uses the same handler dicts from the scope.
4. **Unhandled exceptions:** any exception with no matching handler propagates outward through user middleware to `ServerErrorMiddleware`. It is the outermost layer and receives the `500` or `Exception` handler, or its default debug/500 behavior.

## Not verified

I did not read `starlette/middleware/errors.py`. My search of it returned nothing, so how `ServerErrorMiddleware` handles the error, including whether it re-raises after responding, is unconfirmed.