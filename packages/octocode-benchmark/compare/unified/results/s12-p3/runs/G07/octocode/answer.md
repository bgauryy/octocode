The stack is built lazily on the first request, and exceptions are caught at three layers: the route wrapper, `ExceptionMiddleware`, and `ServerErrorMiddleware`. All line numbers are at commit 63c5760d8a. One caveat is that I haven't read how `Router` dispatches to routes.

## Assembling the middleware stack

- **Lazy build.** `Starlette.__call__` sets `scope["app"]`. If `self.middleware_stack is None`, it calls `build_middleware_stack()`, then calls the stack (`starlette/applications.py:92-96`).
- **Exception handler split.** `build_middleware_stack` (`applications.py:63-83`) goes through `self.exception_handlers`. Keys `500` or `Exception` become the `error_handler` for `ServerErrorMiddleware`. All other keys go into `exception_handlers` for `ExceptionMiddleware` (`:68-72`).
- **Layer order, outermost first** (`:74-78`):
  1. `ServerErrorMiddleware(handler=error_handler, debug=debug)`.
  2. `RequestBodyLimitMiddleware`, but only if `max_body_size is not None`.
  3. The user middleware (`self.user_middleware`).
  4. `ExceptionMiddleware(handlers=exception_handlers, debug=debug)`.
- **Wrapping.** `app = self.router`, then `for cls, args, kwargs in reversed(middleware): app = cls(app, *args, **kwargs)` (`:80-82`). The first item in the list ends up outermost, with the router innermost.
- **`add_middleware`.** It inserts at index 0 of `user_middleware` (`:107`), so the most recently added middleware is the outermost user middleware. It raises `RuntimeError` if the stack already exists (`:105-106`).

## How exceptions reach handlers

1. **Route-level wrapper.** In `starlette/routing.py`, `request_response` wraps the endpoint with `wrap_app_handling_exceptions(app, request)` (`:65`). `websocket_session` does the same for WebSocket endpoints (`:84`).
2. **`ExceptionMiddleware`** (`starlette/middleware/exceptions.py`).
   - **Default handlers.** Its defaults are `HTTPException` → `http_exception` and `WebSocketException` → `websocket_exception` (`:28-31`).
   - **Handler registration.** `add_exception_handler` stores integer keys in `_status_handlers` and class keys in `_exception_handlers` (`:41-45`).
   - **Scope sharing.** For `http` and `websocket` scopes, `__call__` puts both dicts in `scope["starlette.exception_handlers"]` (`:52-55`).
   - **Wrapping.** It then runs `wrap_app_handling_exceptions(self.app, conn)` (`:63`).
3. **`wrap_app_handling_exceptions`** (`starlette/_exception_handler.py:23-65`).
   - **Handler source.** It reads the handlers back from `conn.scope["starlette.exception_handlers"]`, falling back to empty dicts (`:26-29`). That is how the route-level wrapper in step 1 finds the same handlers.
   - **Catching.** It runs the app inside `try/except Exception` (`:41-43`).
   - **Lookup order.** For an `HTTPException` it tries the status-code handler first (`:46-47`). If none is found, it walks `type(exc).__mro__` looking for a registered class (`:50`, `_lookup_exception_handler` at `:16-20`).
   - **No handler.** If there is still no handler, it re-raises (`:52-53`).
   - **Response already started.** If the response has already started, it raises `RuntimeError("Caught handled exception, but response already started.")` (`:55-56`).
   - **Calling the handler.** Async handlers are awaited. Sync handlers run via `run_in_threadpool` (`:58-61`). A non-`None` response is then sent (`:62-63`).
4. **`ServerErrorMiddleware`** (`starlette/middleware/errors.py:149-186`).
   - **Scope.** It only handles `http` scopes.
   - **Catching.** It catches anything that escaped the inner layers, including exceptions raised by user middleware or by handlers.
   - **Response choice.** In debug mode it returns a traceback response. With no custom handler it uses `error_response`. Otherwise it calls the installed 500 or `Exception` handler.
   - **Sending.** It sends the response only if one hasn't started (`:180-181`).
   - **Re-raise.** It always re-raises the exception afterwards (`:183-186`), so servers can log it and test clients can surface it.

## Uncertainty

- **`Router` internals.** I didn't read the `Router` internals, such as how 404 and 405 exceptions are produced. The step 1 wrapper lines come from the pinned commit, but the `ghSearchCode` hit that surfaced them ran against a different commit, 6beaa2e.
- **`errors.py` lines I didn't read.** I didn't read `debug_response` or `error_response`, so the debug-mode and default-500 behavior is taken from the call sites only.