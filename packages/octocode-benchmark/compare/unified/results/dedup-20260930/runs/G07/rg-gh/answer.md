**Short answer:** `Starlette.build_middleware_stack()` builds the stack lazily. The order, outermost first, is `ServerErrorMiddleware`, then an optional body-limit middleware, then the user middleware, then `ExceptionMiddleware`, then the router. Exceptions propagate outward until `wrap_app_handling_exceptions` finds a matching handler. Anything unhandled reaches `ServerErrorMiddleware`, which produces a 500 and re-raises.

I read these files from the GitHub API at `63c5760d8a`, not from a local checkout. Line numbers are from the fetched files.

## Assembly (`starlette/applications.py`)
- **Lazy build:** `__call__` builds the stack on the first call: `if self.middleware_stack is None: self.middleware_stack = self.build_middleware_stack()` (`applications.py:94-96`).
- **Handler split** (`:65-72`):
  - Handlers registered under key `500` or `Exception` become `error_handler`, which goes to `ServerErrorMiddleware`.
  - All other handlers go into the `exception_handlers` dict.
- **Order** (`:74-78`):
  1. `ServerErrorMiddleware(handler=error_handler, debug=debug)`
  2. `RequestBodyLimitMiddleware`, only if `self.max_body_size is not None`
  3. `self.user_middleware`
  4. `ExceptionMiddleware(handlers=exception_handlers, debug=debug)`
- **Wrapping:** the list is wrapped around `self.router` in reverse, so the first entry ends up outermost (`:80-82`).
- **Adding middleware:** `add_middleware` inserts at index 0 of `user_middleware` (`:107`), so the most recently added middleware is outermost among user middleware. It raises if the stack is already built (`:105-106`).

## Exception routing
- **`ExceptionMiddleware`** (`middleware/exceptions.py`):
  - It passes non-http and non-websocket scopes straight through (`:48-50`).
  - Otherwise it stores `(exception_handlers, status_handlers)` in `scope["starlette.exception_handlers"]` (`:52-55`).
  - It then runs `wrap_app_handling_exceptions(self.app, conn)` (`:63`).
  - Integer keys go into `_status_handlers` and class keys into `_exception_handlers` (`:41-45`).
  - Default handlers exist for `HTTPException` and `WebSocketException` (`:28-31`).
- **`wrap_app_handling_exceptions`** (`_exception_handler.py:23-65`):
  - It catches `Exception` around the app (`:41-43`).
  - For an `HTTPException`, it first tries the status-code handler (`:46-47`).
  - Otherwise it walks the exception's `__mro__` to find the nearest class handler (`_lookup_exception_handler`, `:16-20`, called at `:50`).
  - If no handler matches, it re-raises (`:52-53`).
  - If the response has already started, it raises `RuntimeError` (`:55-56`).
  - It awaits async handlers and runs sync handlers in a threadpool (`:58-61`), then sends the returned response (`:62-63`).
- **Router-level wrapping:** `routing.py` also calls `wrap_app_handling_exceptions` per request (`:65`) and per websocket session (`:84`). It reads the handlers that `ExceptionMiddleware` stored in the scope. Exceptions from an endpoint are therefore handled close to the route, inside any user middleware.
- **`ServerErrorMiddleware`** (`middleware/errors.py:149-186`):
  - It only acts on `http` scopes (`:150-152`).
  - On an unhandled `Exception` it picks a response (`:167-178`):
    - In debug mode, a traceback response.
    - With no custom handler, the default 500 response.
    - Otherwise the custom 500 handler.
  - It sends the response only if the response hasn't started (`:180-181`).
  - It always re-raises (`:186`), so servers and test clients still see the error.

## Uncertainty
- I did not open `RequestBodyLimitMiddleware` or the rest of `routing.py` beyond the two call sites.
- I did not check how `Mount` or `Route` interact with handlers beyond those call sites.