**Short answer:** `Starlette.build_middleware_stack()` builds the stack lazily and wraps the router in a fixed order of middleware. Exceptions are then caught at up to three layers. `wrap_app_handling_exceptions` runs per route and inside `ExceptionMiddleware`, and `ServerErrorMiddleware` is the outermost catch-all.

Two caveats on citations. The `ghSearchCode` call that found the `routing.py` call sites reported a different commit (`6beaa2e39d33…`, the default branch) from the pinned `63c5760d8a`. I didn't read `routing.py` at the pinned commit.

## Stack assembly (`starlette/applications.py`)

- **When it's built:** `__call__` sets `scope["app"]`. If `self.middleware_stack is None`, it calls `build_middleware_stack()` once, then calls the stack (`applications.py:92-96`).
- **Handler split:** `build_middleware_stack` loops over `self.exception_handlers` (lines 68-72). Handlers keyed `500` or `Exception` become `error_handler`, which goes to `ServerErrorMiddleware`. All other handlers go to `ExceptionMiddleware`.
- **Order:** the list is built outermost first (lines 74-78):
  1. `ServerErrorMiddleware(handler=error_handler, debug=debug)`.
  2. `RequestBodyLimitMiddleware`, only if `max_body_size` is not `None`.
  3. `self.user_middleware`.
  4. `ExceptionMiddleware(handlers=exception_handlers, debug=debug)`.
- **Wrapping:** `app = self.router`, then `for cls, args, kwargs in reversed(middleware): app = cls(app, *args, **kwargs)` (lines 80-83). Each middleware wraps the next one, so the first in the list ends up outermost.
- **`add_middleware`:** it does `user_middleware.insert(0, …)`, so the most recently added middleware is outermost among the user middleware (line 107). It raises `RuntimeError` if the stack is already built (lines 105-106).

## How exceptions reach handlers

1. **`ExceptionMiddleware`** (`starlette/middleware/exceptions.py`):
   - Non-http/websocket scopes pass straight through (lines 48-50).
   - Its `__init__` registers default handlers for `HTTPException` and `WebSocketException` (lines 28-31).
   - It then calls `add_exception_handler` for each user handler. Integer keys go to `_status_handlers`; class keys go to `_exception_handlers` (lines 36-45).
   - Per request it stores both dicts in `scope["starlette.exception_handlers"]` (lines 52-55). It builds a `Request` or `WebSocket` as `conn` and runs `wrap_app_handling_exceptions(self.app, conn)(scope, receive, send)` (lines 57-63).

2. **`wrap_app_handling_exceptions`** (`starlette/_exception_handler.py:23-65`):
   - It reads the handler dicts from `conn.scope["starlette.exception_handlers"]`, falling back to empty dicts on `KeyError` (lines 26-29).
   - It wraps `send` to track whether `http.response.start` was sent (lines 34-39).
   - On `except Exception`:
     - For an `HTTPException`, it first tries `status_handlers.get(exc.status_code)` (lines 46-47).
     - Otherwise it uses `_lookup_exception_handler`, which walks `type(exc).__mro__` and returns the first class with a registered handler (lines 16-20, 49-50).
     - If no handler is found, it re-raises (lines 52-53).
     - If the response has already started, it raises `RuntimeError("Caught handled exception, but response already started.")` (lines 55-56).
     - Otherwise it calls the handler. Async handlers are awaited; sync handlers go through `run_in_threadpool` (lines 58-61). If the handler returns a response, it is sent (lines 62-63).

3. **The router wraps individual routes too.** `starlette/routing.py` calls `wrap_app_handling_exceptions(app, request)` at line 65 and `wrap_app_handling_exceptions(app, session)` at line 84. These read the same scope key. As a result, handlers can fire at the route level, inside the router.

4. **`ServerErrorMiddleware`** (`starlette/middleware/errors.py:127-186`):
   - It handles only `http` scopes (lines 150-152).
   - It catches any exception that is still unhandled (line 165).
   - It then builds the response in one of three ways (lines 167-178):
     - If `debug` is set, it returns a traceback response.
     - If no handler is installed, it uses the default `error_response`.
     - Otherwise it calls the user's 500/`Exception` handler.
   - It sends that response only if the response hasn't started (lines 180-181).
   - It always re-raises the exception afterwards, so servers can log it and test clients can surface it (lines 183-186).

## Uncertainty
I didn't read `routing.py` at the pinned commit, so the exact context around lines 65 and 84 is unverified. I also didn't read the middle of `errors.py`, which holds `debug_response` and `error_response`.