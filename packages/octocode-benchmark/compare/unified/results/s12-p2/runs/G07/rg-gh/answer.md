**Short answer:** `Starlette.build_middleware_stack` wraps the router in a fixed-order chain of middleware, and it builds that chain lazily on the first call. Exceptions are caught by `ExceptionMiddleware` and by per-route wrappers. Both use `wrap_app_handling_exceptions`. Anything left unhandled, plus handlers for `500` or `Exception`, goes to `ServerErrorMiddleware` at the outside.

I read the files through the GitHub API at 63c5760d8a. The line numbers below come from that output. The `applications.py` numbers are relative to a slice starting at line 62, so they're approximate.

**Stack assembly** (`starlette/applications.py`, `build_middleware_stack`, about lines 63–84)
- `Starlette.__init__` stores `self.user_middleware` and `self.exception_handlers`, and sets `self.middleware_stack = None` (lines 59–61).
- `__call__` builds the stack on the first request if it's still `None`, then calls it (lines 94–96).
- Handlers keyed `500` or `Exception` become the `error_handler` for `ServerErrorMiddleware`. All other handlers go to `ExceptionMiddleware`.
- The list is ordered like this:
  1. `ServerErrorMiddleware(handler=error_handler, debug=debug)` is outermost.
  2. `RequestBodyLimitMiddleware` comes next, but only if `max_body_size` is set.
  3. The user middleware follows, in the order given.
  4. `ExceptionMiddleware(handlers=exception_handlers, debug=debug)` is innermost.
- The code starts with `app = self.router` and then loops over `reversed(middleware)`, doing `app = cls(app, *args, **kwargs)`. The first item in the list therefore ends up outermost.
- `add_middleware` inserts at index 0 of `user_middleware` (line 107), so the most recently added middleware is outermost among the user middleware. It raises a `RuntimeError` if the stack has already been built (lines 105–106).

**Exception routing**
- **`ExceptionMiddleware`** (`starlette/middleware/exceptions.py`):
  - It registers defaults for `HTTPException` and `WebSocketException` (lines 28–31).
  - `add_exception_handler` sorts handlers into `_status_handlers` for integer keys and `_exception_handlers` for classes (lines 41–45).
  - For `http` and `websocket` scopes, it puts both dicts into `scope["starlette.exception_handlers"]` (lines 52–55). It then runs `wrap_app_handling_exceptions(self.app, conn)` (line 63).
- **`wrap_app_handling_exceptions`** (`starlette/_exception_handler.py:23-65`):
  - It reads the handler dicts from the scope (lines 26–29).
  - It wraps `send` to track whether the response has started (lines 34–39).
  - On `except Exception` (line 43), it looks up a handler in this order:
    - For an `HTTPException`, it first tries the status-code handler (lines 46–47).
    - Otherwise it walks the exception's MRO in `_lookup_exception_handler` (lines 16–20, 50).
  - If it finds no handler, it re-raises (lines 52–53).
  - If the response has already started, it raises a `RuntimeError` (lines 55–56).
  - Otherwise it calls the handler directly if it's async, or through `run_in_threadpool` if not (lines 58–61). It then sends the returned response (lines 62–63).
- **Routes** (`starlette/routing.py:65` and `:84`) call `wrap_app_handling_exceptions` again around each request or websocket endpoint. Handlers therefore also apply inside the router, using the same scope entry.
- **`ServerErrorMiddleware`** (`starlette/middleware/errors.py`, about lines 149–186):
  - It catches whatever the inner layers didn't handle. That includes exceptions from user middleware that sit outside `ExceptionMiddleware`, and exceptions raised by handlers themselves.
  - It uses the custom `500` or `Exception` handler if there is one, and otherwise its default 500 response (lines 170–178).
  - It sends the response only if one hasn't started (line 180).
  - It then re-raises the exception (line 186), so the server and test client still see it.

**Uncertainty:** I only grepped `errors.py` and `routing.py`, so I haven't read the debug-mode traceback rendering or the surrounding route code. I haven't confirmed the `applications.py` line numbers precisely.