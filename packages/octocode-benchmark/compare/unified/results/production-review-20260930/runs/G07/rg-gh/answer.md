The stack is built lazily and wrapped from the outside in. Exceptions are caught by `ExceptionMiddleware` and by a per-route wrapper, with `ServerErrorMiddleware` as the outermost catch-all. I read these files through `gh api` at 63c5760d8a. The paths are under `starlette/`.

## Assembling the stack

- **State set in `__init__`:** `self.exception_handlers` and `self.user_middleware` are copied from the constructor arguments, and `self.middleware_stack` starts as `None` (`applications.py:59-61`).
- **When it is built:** the first `Starlette.__call__` sets `scope["app"]` and calls `build_middleware_stack()` if the stack is `None` (`applications.py:92-96`). The stack is then cached.
- **Splitting the handlers:** `build_middleware_stack()` (`applications.py:63-83`) sends handlers keyed `500` or `Exception` to `error_handler`. All other handlers go into `exception_handlers` (`applications.py:68-72`).
- **Order, outermost first** (`applications.py:74-78`):
  1. `ServerErrorMiddleware(handler=error_handler, debug=debug)`
  2. `RequestBodyLimitMiddleware`, only if `max_body_size` is set
  3. the user middleware, in list order
  4. `ExceptionMiddleware(handlers=exception_handlers, debug=debug)`
- **Nesting:** the innermost app is `self.router`. The list is wrapped in reverse, so each `cls(app, *args, **kwargs)` wraps the previous result (`applications.py:80-83`).
- **Adding middleware later:** `add_middleware` inserts at index 0 of `user_middleware`, so the most recently added middleware is outermost among the user middleware. It raises `RuntimeError` if the stack is already built (`applications.py:104-107`).

## How exceptions reach handlers

1. **`ExceptionMiddleware`** (`middleware/exceptions.py:18-63`)
   - Its `_exception_handlers` start with `HTTPException` and `WebSocketException` handlers, and the user's handlers are added on top (`:28-34`).
   - `add_exception_handler` puts integer keys in `_status_handlers` and exception classes in `_exception_handlers` (`:41-45`).
   - For `http` and `websocket` scopes, `__call__` stores both dicts in `scope["starlette.exception_handlers"]` (`:52-55`). It builds a `Request` or `WebSocket` and runs `wrap_app_handling_exceptions(self.app, conn)` (`:57-63`). Other scope types pass straight through (`:48-50`).
   - The default `http_exception` returns a `PlainTextResponse`, or an empty `Response` for 204 and 304 (`:65-69`).

2. **`wrap_app_handling_exceptions`** (`_exception_handler.py:23-65`)
   - It reads the handler dicts from `scope["starlette.exception_handlers"]` and falls back to empty dicts (`:26-29`).
   - It wraps `send` to track whether `http.response.start` has gone out (`:34-39`).
   - On `except Exception`, handler lookup goes in this order:
     - For an `HTTPException`, it first tries the handler for that status code (`:46-47`).
     - Otherwise it walks `type(exc).__mro__` and takes the first class with a registered handler (`:16-20`, `:50`).
     - If no handler is found, it re-raises (`:52-53`).
   - If the response has already started, it raises `RuntimeError` (`:55-56`).
   - Otherwise it calls the handler directly if async, or in a threadpool if not (`:58-61`). Any returned response is sent (`:62-63`).

3. **Per-route wrapping** (`routing.py:16`, `:65`, `:84`)
   - `request_response` and `websocket_session` also call `wrap_app_handling_exceptions(app, request/session)`.
   - Because of this, an exception in an endpoint is handled inside the router. Middleware between the router and `ExceptionMiddleware` therefore doesn't see exceptions that a registered handler deals with. The route-level wrapper reads the dicts that `ExceptionMiddleware` put in the scope.

4. **`ServerErrorMiddleware`** (`middleware/errors.py:149-186`) is the outermost catch-all. It only handles `http` scopes (`:150-152`).
   - For any exception that reaches it, it picks the response in this order: a traceback response if `debug` is set (`:167-169`), the default `error_response` if no handler is installed (`:170-172`), or the user's `500`/`Exception` handler (`:173-178`).
   - It sends the response only if one hasn't already started (`:180-181`).
   - It then always re-raises the exception, so servers can log it and test clients can surface it (`:183-186`).

## Uncertainty
I didn't read the `Middleware` class in `middleware/__init__.py`. I assumed it unpacks as `(cls, args, kwargs)`, which is how `applications.py:81` uses it. I also didn't open `routing.py` beyond the grep hits above.