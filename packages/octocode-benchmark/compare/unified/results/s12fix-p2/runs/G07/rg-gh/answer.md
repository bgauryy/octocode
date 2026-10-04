The middleware stack is built lazily on the first call. Raised exceptions are caught by `ExceptionMiddleware` for registered handlers, or by `ServerErrorMiddleware` for 500s and unhandled errors. I read the source through the GitHub API at 63c5760d8a. I didn't run anything, and I didn't read `Middleware.__iter__` in full.

**Stack assembly** (`starlette/applications.py`)
- `Starlette.__init__` stores `exception_handlers` and `user_middleware` (`:59-61`). It sets `middleware_stack = None`.
- `__call__` sets `scope["app"]`. If `middleware_stack` is `None`, it calls `build_middleware_stack()` and then calls the result (`:92-96`).
- `build_middleware_stack` (`:63-83`):
  - It splits the handlers. Any handler keyed `500` or `Exception` becomes `error_handler`. The rest go to `exception_handlers` (`:68-72`).
  - It builds the middleware list in this order (`:74-78`):
    1. `ServerErrorMiddleware(handler=error_handler, debug=debug)` is outermost.
    2. `RequestBodyLimitMiddleware` comes next, only if `max_body_size` is set.
    3. The user middleware follows, in the order given.
    4. `ExceptionMiddleware(handlers=exception_handlers, debug=debug)` is innermost.
  - It starts from `app = self.router` and wraps in reverse order with `cls(app, *args, **kwargs)`, so the first list entry ends up outermost (`:80-82`).
- `add_middleware` inserts at index 0 of `user_middleware`, so the most recently added middleware is outermost among the user middleware (`:104-107`). It raises `RuntimeError` if the stack is already built, though that line is marked `# pragma: no cover`.
- `Middleware` is a `(cls, args, kwargs)` holder. It defines `__iter__`, which is what lets the loop unpack it (`starlette/middleware/__init__.py:21-29`).

**How exceptions reach handlers**
1. **`ExceptionMiddleware`** (`starlette/middleware/exceptions.py`):
   - Its constructor registers default handlers for `HTTPException` and `WebSocketException`, then adds the user handlers (`:28-34`).
   - `add_exception_handler` puts `int` keys in `_status_handlers` and exception classes in `_exception_handlers` (`:41-45`).
   - For `http` and `websocket` scopes, `__call__` stores both dicts in `scope["starlette.exception_handlers"]`. It then runs `wrap_app_handling_exceptions(self.app, conn)` (`:47-63`).
2. **`wrap_app_handling_exceptions`** (`starlette/_exception_handler.py:23-65`):
   - It wraps the app in a try/except. A `sender` wrapper records whether `http.response.start` has been sent (`:32-39`).
   - On an `Exception` it looks for a handler (`:43-50`):
     - For an `HTTPException`, it first looks up the status code in `status_handlers`.
     - Otherwise it walks `type(exc).__mro__` and picks the first class that has a handler (`_lookup_exception_handler`, `:16-20`).
   - If no handler is found, it re-raises (`:52-53`).
   - If a handler is found but the response has already started, it raises `RuntimeError` (`:55-56`).
   - Otherwise it calls the handler. Async handlers are awaited directly and sync ones run in a threadpool. The returned response is then sent (`:58-63`).
3. **Per-route wrapping:** `routing.py` also calls `wrap_app_handling_exceptions` in `request_response` (`:65`) and `websocket_session` (`:84`). It reads the handlers from the same scope key, so exceptions raised inside an endpoint are handled at the route level. Handlers from `ExceptionMiddleware` apply there too. I only checked the call sites in `routing.py`, not the surrounding code.
4. **`ServerErrorMiddleware`** (`starlette/middleware/errors.py:150-186`) is the outer catch-all for HTTP:
   - It catches any `Exception` that wasn't handled further in.
   - It builds the 500 response in one of three ways:
     - the debug traceback response, if `debug` is on (`:167-169`);
     - the default "Internal Server Error" text (`:170-172`);
     - the custom `500` or `Exception` handler (`:174-178`).
   - It sends the response only if one hasn't started (`:180-181`).
   - It always re-raises (`:183-186`), so the server can log the error and the test client can surface it.