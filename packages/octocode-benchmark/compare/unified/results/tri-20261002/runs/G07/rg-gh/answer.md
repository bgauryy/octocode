**Short answer:** The stack is built lazily on the first call. The order is `ServerErrorMiddleware`, then an optional body-limit middleware, then the user middleware, then `ExceptionMiddleware`, then the router. Exceptions are caught by `ExceptionMiddleware` for registered handlers, and by `ServerErrorMiddleware` for 500s and anything left over.

I read the code through `gh api` at 63c5760d8a. Line numbers are from the files at that commit.

**Assembly** (`starlette/applications.py`)
- `__call__` builds the stack on the first request and caches it in `self.middleware_stack` (lines 94–96).
- `build_middleware_stack` (lines 63–83) first splits `self.exception_handlers`:
  - Handlers keyed `500` or `Exception` become `error_handler`, which goes to `ServerErrorMiddleware` (lines 68–72).
  - All other handlers go to `ExceptionMiddleware`.
- It then builds the list in this order (lines 74–78):
  1. `ServerErrorMiddleware(handler=error_handler, debug=debug)`, outermost.
  2. `RequestBodyLimitMiddleware(max_body_size=...)`, only if `self.max_body_size` is set.
  3. `self.user_middleware`.
  4. `ExceptionMiddleware(handlers=exception_handlers, debug=debug)`, innermost.
- It wraps `self.router` by looping over the list in reverse and calling `cls(app, *args, **kwargs)` (lines 80–83). The first item in the list therefore ends up outermost.
- `add_middleware` inserts at position 0 of `user_middleware` (line 107), so the most recently added middleware is the outermost of the user middleware. It raises `RuntimeError` if the stack has already been built (lines 105–106).

**Exception routing**
- `ExceptionMiddleware.__call__` (`starlette/middleware/exceptions.py:47-63`):
  - It ignores scope types other than http and websocket.
  - It stores `(exception_handlers, status_handlers)` in `scope["starlette.exception_handlers"]`.
  - It then runs `wrap_app_handling_exceptions(self.app, conn)`.
- Its handler tables are built at lines 27–34 and in `add_exception_handler` (lines 36–45):
  - Integer keys go into the status-code table.
  - Class keys go into the exception-class table.
  - `HTTPException` and `WebSocketException` have built-in handlers.
- `wrap_app_handling_exceptions` (`starlette/_exception_handler.py:23-65`) wraps the app in a `try/except Exception` (lines 41–43). It looks up a handler in this order:
  1. For an `HTTPException`, it tries the status-code handler first (lines 46–47).
  2. Otherwise it walks `type(exc).__mro__` and takes the first class with a registered handler (`_lookup_exception_handler`, lines 16–20, called at line 50).
  3. If no handler is found, it re-raises (lines 52–53).
- If a handler is found:
  - If the response has already started, it raises `RuntimeError` (lines 55–56).
  - Otherwise it calls the handler directly if it is async, or through `run_in_threadpool` if not (lines 58–61).
  - It then sends the returned response (lines 62–63).
- Because the wrapper stores the handler tables in the scope, the same wrapper function can be reused elsewhere. It falls back to empty tables if the key is missing (lines 26–29). I did not check where else it is called.
- Unhandled exceptions, and exceptions raised by the user middleware sitting outside `ExceptionMiddleware`, reach `ServerErrorMiddleware` (`starlette/middleware/errors.py:150-186`):
  - It only handles http scopes.
  - On an exception it builds the response in one of three ways:
    - In debug mode it returns a traceback response (HTML or plain text) with status 500.
    - With no custom handler it returns "Internal Server Error" with status 500.
    - With a custom handler it uses the handler's response.
  - It sends the response only if the response hasn't started.
  - It then always re-raises the exception (lines 180–186), so the server can log it and the test client can surface it.

**Uncertainty:** I read only `applications.py`, `middleware/exceptions.py`, `_exception_handler.py` and the `ServerErrorMiddleware` `__call__` section of `middleware/errors.py`. I did not check how `Router` or routes use `wrap_app_handling_exceptions`, or how `BaseHTTPMiddleware` interacts with these layers.