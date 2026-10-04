**Short answer:** `Starlette.build_middleware_stack()` builds the stack lazily on the first request. Exceptions are caught by `ExceptionMiddleware` (and per-route wrappers) for registered handlers, and by `ServerErrorMiddleware` for everything else.

## Stack assembly (`starlette/applications.py`)

- `__call__` sets `scope["app"] = self`. If `self.middleware_stack` is `None`, it calls `build_middleware_stack()` once, then delegates to the stack (`applications.py:92-96`).
- `build_middleware_stack` (`applications.py:63-83`) first splits `self.exception_handlers`. Keys `500` or `Exception` become `error_handler` for the outer middleware. All other keys go into `exception_handlers` (`:68-72`).
- It then builds the middleware list in this order (`:74-78`):
  1. `ServerErrorMiddleware(handler=error_handler, debug=debug)` is outermost.
  2. `RequestBodyLimitMiddleware` is added only if `max_body_size` is not `None`.
  3. The user middleware (`self.user_middleware`).
  4. `ExceptionMiddleware(handlers=exception_handlers, debug=debug)` is innermost.
- The list is wrapped around `self.router` by iterating in `reversed(middleware)` and doing `app = cls(app, *args, **kwargs)` (`:80-82`). The first entry therefore ends up outermost.
- `add_middleware` does `user_middleware.insert(0, ...)`, so the most recently added middleware is outermost among the user middleware (`:107`). It raises `RuntimeError` if the stack has already been built (`:105-106`).

## How exceptions reach handlers

1. **`ExceptionMiddleware`** (`starlette/middleware/exceptions.py`):
   - Its handler tables start with defaults for `HTTPException` and `WebSocketException` (`:28-31`).
   - `add_exception_handler` sends int keys to `_status_handlers` and exception classes to `_exception_handlers` (`:36-45`).
   - For `http` and `websocket` scopes, `__call__` stores both tables in `scope["starlette.exception_handlers"]`. It then builds a `Request` or `WebSocket` and runs `wrap_app_handling_exceptions(self.app, conn)` (`:47-63`). Other scope types pass straight through.

2. **`wrap_app_handling_exceptions`** (`starlette/_exception_handler.py:23-65`):
   - It reads the tables from `scope["starlette.exception_handlers"]`, falling back to empty dicts (`:26-29`).
   - It wraps `send` to track whether `http.response.start` has been sent (`:34-39`).
   - On `except Exception`, it picks a handler in this order (`:44-53`):
     - For an `HTTPException`, it first looks up `status_handlers[exc.status_code]`.
     - Otherwise it calls `_lookup_exception_handler`, which walks `type(exc).__mro__` and returns the first class that has a handler (`:16-20`).
     - If no handler is found, it re-raises.
   - If the response has already started, it raises `RuntimeError("Caught handled exception, but response already started.")` (`:55-56`).
   - Otherwise it awaits the handler if it is async, or runs it via `run_in_threadpool` if not. A non-`None` result is then sent as the response (`:58-63`).

3. **Per-route wrapping:** `starlette/routing.py` also calls `wrap_app_handling_exceptions` at lines 65 and 84 (a request wrapper and a websocket-session wrapper). I only saw these in a code search, not the surrounding code. They apparently let handlers run inside the router, using the same scope tables. The search ran against a different commit (`6beaa2e`), so line numbers at `63c5760d8a` may differ.

4. **`ServerErrorMiddleware`** (`starlette/middleware/errors.py:127-186`) is the last resort. It only handles `http` scopes (`:150`). On any `Exception` that reached it:
   - It builds the response in one of three ways: a debug traceback if `debug` is set, the default `error_response` if no handler is installed, or the user's `500`/`Exception` handler (`:167-178`).
   - It sends that response only if the response hasn't started (`:180-181`).
   - It then always re-raises the exception, so servers can log it and test clients can surface it (`:183-186`).

## Caveats
- I did not read `routing.py` at the pinned commit, so the exact role of its two `wrap_app_handling_exceptions` call sites is inferred from the search hits.
- I did not read `RequestBodyLimitMiddleware`'s own error behavior.