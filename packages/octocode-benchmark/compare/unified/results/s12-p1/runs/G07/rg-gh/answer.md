The middleware stack is built lazily on the first request. Exceptions are handled in two layers: `ExceptionMiddleware` takes the handlers registered for specific exception classes or status codes, and `ServerErrorMiddleware` takes everything else as a 500. I read the code through the GitHub API at 63c5760d8a, not a local checkout.

## Stack assembly

- **Trigger:** `Starlette.__call__` sets `scope["app"]`. If `self.middleware_stack is None`, it calls `build_middleware_stack()` and then calls the resulting stack (`starlette/applications.py:92-96`).
- **Handler split:** `build_middleware_stack` (`applications.py:63-83`) separates the handlers in `self.exception_handlers`.
  - Handlers keyed `500` or `Exception` become `error_handler`, which goes to `ServerErrorMiddleware` (`:68-72`).
  - All other handlers go to `ExceptionMiddleware`.
- **Order, outermost to innermost** (`:74-78`):
  1. `ServerErrorMiddleware(handler=error_handler, debug=debug)`
  2. `RequestBodyLimitMiddleware`, only if `max_body_size` is set (`:75-76`)
  3. `self.user_middleware`
  4. `ExceptionMiddleware(handlers=exception_handlers, debug=debug)`
  5. `self.router`
- **Wrapping:** The code starts with `app = self.router` and loops over `reversed(middleware)`, doing `app = cls(app, *args, **kwargs)` (`:80-82`). The first item in the list therefore ends up outermost.
- **`add_middleware`:** It inserts at position 0 of `user_middleware` (`:107`), so the most recently added middleware is the outermost of the user middleware. It raises `RuntimeError` if the stack has already been built (`:105-106`).

## Exception routing

1. **Registration:** `ExceptionMiddleware.__init__` registers default handlers for `HTTPException` and `WebSocketException`, then adds the user's handlers (`starlette/middleware/exceptions.py:28-34`).
   - Integer keys go into `_status_handlers`.
   - Class keys go into `_exception_handlers` (`:41-45`).
2. **Scope hand-off:** For `http` and `websocket` scopes, it stores both dicts in `scope["starlette.exception_handlers"]` (`:52-55`). It then runs `wrap_app_handling_exceptions(self.app, conn)(scope, receive, send)` (`:63`).
3. **Lookup:** `wrap_app_handling_exceptions` (`starlette/_exception_handler.py:23-65`) wraps the app in a `try/except Exception` (`:41-43`).
   - For an `HTTPException`, it first checks `status_handlers` by status code (`:46-47`).
   - Otherwise it walks `type(exc).__mro__` and takes the first class with a registered handler (`_lookup_exception_handler`, `:16-20`, called at `:49-50`). A handler for a base class therefore catches subclasses.
   - If no handler is found, it re-raises (`:52-53`).
4. **Running the handler:**
   - If the response has already started, it raises `RuntimeError("Caught handled exception, but response already started.")` (`:55-56`).
   - Otherwise it awaits the handler if it is async, or runs it in a threadpool if not (`:58-61`).
   - It then sends the returned response (`:62-63`).
5. **Per-route wrapping:** `starlette/routing.py` also calls `wrap_app_handling_exceptions` at lines 65 and 84. These look like the request and websocket-session wrappers for individual routes, but I only grepped for the calls and didn't read that code. The same handlers are found through the scope entry set in step 2.
6. **Unhandled exceptions:** These reach the outermost `ServerErrorMiddleware.__call__` (`starlette/middleware/errors.py:150-186`).
   - In debug mode it returns a traceback response (`:167-169`).
   - Otherwise it uses the custom 500 handler if one is registered (`:174-178`), or a default "Internal Server Error" plain-text 500 (`:170-172`, `:258-259`).
   - It sends the response only if one hasn't already started (`:180-181`).
   - It always re-raises the exception afterwards, so servers can log it and test clients can surface it (`:183-186`).

## Uncertainty
I didn't read `Middleware.__iter__` or the `Router` internals. I only confirmed that `class Middleware` is at `starlette/middleware/__init__.py:21`. The tuple unpacking `for cls, args, kwargs in ...` at `applications.py:81` implies it yields `(cls, args, kwargs)`.