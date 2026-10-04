**Short answer:** `Starlette.build_middleware_stack` builds the stack lazily on the first request. Exceptions are caught by `wrap_app_handling_exceptions`, which looks up a handler. If none is found, or the exception is a 500, `ServerErrorMiddleware` handles it.

## Stack assembly (`starlette/applications.py`, commit 63c5760d8a)

- `__call__` sets `scope["app"]`. If `self.middleware_stack is None`, it calls `build_middleware_stack()` and caches the result (lines 92–96).
- `build_middleware_stack` (lines 63–83) works in three steps:
  1. It splits `self.exception_handlers`. A handler keyed `500` or `Exception` becomes `error_handler`, which goes to `ServerErrorMiddleware`. All other handlers go to `ExceptionMiddleware` (lines 68–72).
  2. It builds the middleware list in this order (lines 74–78):
     1. `ServerErrorMiddleware(handler=error_handler, debug=debug)`.
     2. `RequestBodyLimitMiddleware`, only if `max_body_size` is set.
     3. The user middleware (`self.user_middleware`).
     4. `ExceptionMiddleware(handlers=exception_handlers, debug=debug)`.
  3. It wraps `self.router` from the inside out: `for cls, args, kwargs in reversed(middleware): app = cls(app, *args, **kwargs)` (lines 80–83). The first item in the list ends up outermost.
- `add_middleware` does `user_middleware.insert(0, ...)`, so the most recently added middleware is outermost among user middleware (line 107). It raises `RuntimeError` if the stack is already built, but that branch is marked `# pragma: no cover`.

The resulting request path is: ServerErrorMiddleware → [body limit] → user middleware → ExceptionMiddleware → Router → route.

## How exceptions reach handlers

1. **`ExceptionMiddleware`** (`starlette/middleware/exceptions.py`):
   - Its `_exception_handlers` starts with `HTTPException` and `WebSocketException` defaults (lines 28–31).
   - `add_exception_handler` sorts handlers by key type. An `int` key goes to `_status_handlers`. A class key goes to `_exception_handlers` (lines 41–45).
   - For `http` and `websocket` scopes only, `__call__` stores both dicts in `scope["starlette.exception_handlers"]` (lines 52–55). It then builds a `Request` or `WebSocket` and runs `wrap_app_handling_exceptions(self.app, conn)` (lines 57–63).
2. **`wrap_app_handling_exceptions`** (`starlette/_exception_handler.py:23-65`) reads the handlers from the scope key, or uses empty dicts if the key is missing (lines 26–29). Its `wrapped_app` calls the app inside `try/except Exception` (lines 41–43) and then picks a handler:
   - For an `HTTPException`, it first tries `status_handlers.get(exc.status_code)` (lines 46–47).
   - Otherwise it calls `_lookup_exception_handler`, which walks `type(exc).__mro__` and returns the first class that has a handler (lines 16–20, 49–50).
   - If no handler is found, it re-raises the exception (lines 52–53).
   - If the response has already started (tracked by a wrapped `send`), it raises `RuntimeError("Caught handled exception, but response already started.")` (lines 55–56).
   - Otherwise it awaits an async handler, or runs a sync handler with `run_in_threadpool`. It then sends the returned response if it isn't `None` (lines 58–63).
3. **Routes wrap themselves too.** `starlette/routing.py` imports `wrap_app_handling_exceptions` and applies it at line 65 (request/response endpoints) and line 84 (websocket sessions). Exceptions raised in an endpoint are therefore handled right at the route, using the handlers that `ExceptionMiddleware` put in the scope.
4. **`ServerErrorMiddleware`** (`starlette/middleware/errors.py:149-186`) is the outermost catch-all and only handles `http` scopes. On any exception that reaches it:
   - It builds the response from `debug_response` if `debug` is on. Otherwise it uses the installed 500/`Exception` handler, or the default `error_response` if there is none (lines 167–178).
   - It sends that response only if the response hasn't already started (lines 180–181).
   - It then always re-raises, so the server can log the error or the test client can surface it (lines 183–186).

## Uncertainty

I read `routing.py` only around the `wrap_app_handling_exceptions` matches, so lines 20–61 and 69–80 were not shown. I did not read the other `Router` and `Route` internals.