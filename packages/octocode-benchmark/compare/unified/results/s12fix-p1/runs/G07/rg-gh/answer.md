**Short answer:** The stack is built lazily on the first request. `ServerErrorMiddleware` goes outermost, your middleware goes in the middle, and `ExceptionMiddleware` goes innermost around the router. Exceptions raised below `ExceptionMiddleware` are looked up in the handler tables it installs. Anything unhandled, and any `500` or `Exception` handler, ends up in `ServerErrorMiddleware`.

**Assembly** (`starlette/applications.py`)
- `Starlette.__call__` sets `scope["app"]`. If `middleware_stack` is `None`, it calls `build_middleware_stack()` and then calls the resulting stack (`applications.py:90-96`).
- `build_middleware_stack` (`applications.py:63-85`) does the following:
  - It splits `self.exception_handlers`. Keys `500` and `Exception` become `error_handler`, which is passed to `ServerErrorMiddleware`. All other keys go to `ExceptionMiddleware` (`:67-72`).
  - It builds the list in this order (`:74-78`): `ServerErrorMiddleware`, then `RequestBodyLimitMiddleware` if `max_body_size` is set, then `self.user_middleware`, then `ExceptionMiddleware`.
  - It wraps `self.router` by looping over `reversed(middleware)` and calling `cls(app, *args, **kwargs)` (`:80-83`). The first list item therefore ends up outermost.
- `add_middleware` inserts at index 0 of `user_middleware` (`:104-107`). The most recently added middleware is therefore the outermost of the user middleware. It raises `RuntimeError` if the stack is already built.

**Exception routing**
1. `ExceptionMiddleware.__call__` (`starlette/middleware/exceptions.py:47-63`):
   - It passes through any scope that isn't `http` or `websocket`.
   - It stores `(self._exception_handlers, self._status_handlers)` in `scope["starlette.exception_handlers"]`.
   - It then runs `wrap_app_handling_exceptions(self.app, conn)`.
   - The default handlers cover `HTTPException` and `WebSocketException` (`:28-31`).
   - Integer keys go to the status handlers and class keys go to the exception handlers (`:36-45`).
2. `wrap_app_handling_exceptions` (`starlette/_exception_handler.py:23-65`) catches `Exception` around the app call:
   - For an `HTTPException`, it first tries `status_handlers[exc.status_code]` (`:46-47`).
   - Otherwise it uses `_lookup_exception_handler`, which walks `type(exc).__mro__`, so the most specific class wins (`:16-20`, `:49-50`).
   - If no handler is found, it re-raises (`:52-53`).
   - If the response has already started, it raises `RuntimeError` (`:55-56`).
   - Otherwise it calls the handler (awaited directly if async, otherwise via `run_in_threadpool`) and sends the returned response (`:58-63`).
3. The same wrapper is also applied per route and per websocket session in `starlette/routing.py:65` and `:84`. It reads the handlers that `ExceptionMiddleware` put in the scope, so handlers are applied close to the endpoint.
4. `ServerErrorMiddleware.__call__` (`starlette/middleware/errors.py:149-186`) is the last catch for `http` scopes:
   - It catches any exception that wasn't handled further in.
   - It builds a response in one of three ways: a debug traceback if `debug` is set, the default 500 response, or your `500`/`Exception` handler.
   - It sends that response only if the response hasn't already started.
   - It always re-raises the exception afterwards, so the server can log it and test clients can see it (`:183-186`).

**Uncertainty:** I read only the sections above. I did not check `Middleware` or `_MiddlewareFactory` in `starlette/middleware/__init__.py`, or the `RequestBodyLimitMiddleware` internals.