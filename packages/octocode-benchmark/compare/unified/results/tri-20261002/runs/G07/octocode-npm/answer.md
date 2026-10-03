**Short answer:** `Starlette.build_middleware_stack()` wraps the router in a list of middleware, and it builds that stack lazily on the first request. Exceptions are handled at two levels. `ExceptionMiddleware` and the per-route wrapper `wrap_app_handling_exceptions` handle registered exceptions. `ServerErrorMiddleware` is the outermost catch-all for 500s.

I read the files at commit 63c5760d8a. The tool output didn't include line numbers for most of these files, so I cite functions and the line numbers that were returned (`routing.py` lines 16, 65 and 84).

## Stack assembly (`starlette/applications.py`)

- **When it's built:** `Starlette.__call__` sets `scope["app"] = self`. If `self.middleware_stack is None`, it calls `build_middleware_stack()` and caches the result. It then calls `self.middleware_stack(scope, receive, send)`.
- **Locked after start:** `add_middleware` raises `RuntimeError("Cannot add middleware after an application has started")` once the stack exists. Otherwise it does `user_middleware.insert(0, ...)`, so the most recently added middleware ends up outermost among the user middleware.
- **Splitting the handlers:** `build_middleware_stack()` loops over `self.exception_handlers`. A handler keyed `500` or `Exception` becomes `error_handler`, which goes to `ServerErrorMiddleware`. All other handlers go to `ExceptionMiddleware`.
- **Order, outermost to innermost:**
  1. `ServerErrorMiddleware(handler=error_handler, debug=debug)`
  2. `RequestBodyLimitMiddleware(max_body_size=...)`, only if `max_body_size` is not `None`
  3. the user middleware, in list order
  4. `ExceptionMiddleware(handlers=exception_handlers, debug=debug)`
  5. `self.router`
- **How it's wrapped:** the code does `app = self.router`, then `for cls, args, kwargs in reversed(middleware): app = cls(app, *args, **kwargs)`.

## How exceptions reach handlers

1. **`ExceptionMiddleware.__call__`** (`starlette/middleware/exceptions.py`)
   - It passes through any scope that isn't `http` or `websocket`.
   - It stores `scope["starlette.exception_handlers"] = (self._exception_handlers, self._status_handlers)`.
   - It builds a `Request` or `WebSocket` and runs `wrap_app_handling_exceptions(self.app, conn)(scope, receive, send)`.
   - Default handlers are registered for `HTTPException` (`http_exception`) and `WebSocketException` (`websocket_exception`). `add_exception_handler` sends int keys to `_status_handlers` and class keys to `_exception_handlers`.
   - `http_exception` returns an empty `Response` for status 204 or 304. Otherwise it returns a `PlainTextResponse` with the exception's detail.

2. **`wrap_app_handling_exceptions`** (`starlette/_exception_handler.py`)
   - It reads the handler tables from `conn.scope["starlette.exception_handlers"]`, falling back to empty dicts on `KeyError`.
   - It wraps `send` to track whether `http.response.start` has been sent.
   - It runs `await app(scope, receive, sender)` inside `except Exception as exc`.
   - **Handler lookup:**
     - For an `HTTPException`, it first tries `status_handlers.get(exc.status_code)`.
     - If that finds nothing, `_lookup_exception_handler` walks `type(exc).__mro__` and returns the first class found in the exception handlers, so subclass handlers take precedence.
     - If there is still no handler, it re-raises the exception.
   - **Calling the handler:**
     - If the response has already started, it raises `RuntimeError("Caught handled exception, but response already started.")`.
     - Otherwise it awaits an async handler, or runs a sync handler in a threadpool.
     - If the handler returns a response, it sends it with `await response(scope, receive, sender)`.

3. **Route-level wrapping** (`starlette/routing.py`, lines 16, 65 and 84)
   - Request/response endpoints and websocket sessions also call `wrap_app_handling_exceptions(app, request_or_session)(scope, receive, send)`, at lines 65 and 84.
   - Handled exceptions are therefore caught right at the endpoint, using the same handler tables from the scope. This means exceptions raised inside a mounted app or route are handled there, not only at the app-level `ExceptionMiddleware`.

4. **Unhandled exceptions: `ServerErrorMiddleware.__call__`** (`starlette/middleware/errors.py`)
   - It only handles `http` scopes and passes the others through.
   - On any `Exception`, it builds the response in one of three ways:
     - In debug mode, it builds a traceback response via `debug_response` in a threadpool.
     - With no custom handler, it uses the default `error_response`.
     - Otherwise it calls the installed 500 or `Exception` handler.
   - It sends that response only if the response hasn't started.
   - It then always re-raises the exception, so servers can log it and test clients can surface it.

## Uncertainty
I did not read the rest of `routing.py`, so I haven't checked how `Router` and `Mount` pass scopes down. I also did not read `Middleware` in `starlette/middleware/__init__.py`. I rely only on how it is unpacked in `build_middleware_stack`.