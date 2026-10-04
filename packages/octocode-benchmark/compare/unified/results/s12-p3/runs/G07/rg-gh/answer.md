**Summary:** `Starlette` builds its middleware stack lazily on the first call. It wraps the router in `ServerErrorMiddleware` on the outside and `ExceptionMiddleware` on the inside. Handled exceptions are looked up in a per-request handler table and turned into responses. Unhandled exceptions, and any exception with a 500 or `Exception` handler, go to `ServerErrorMiddleware`. I read these files through the GitHub API at `63c5760d8a`, not from a local checkout.

## Stack assembly (`starlette/applications.py`)

- **Trigger:** `__init__` only stores `user_middleware` and `exception_handlers`, and sets `middleware_stack = None` (`:59-61`). `__call__` sets `scope["app"]`, then calls `build_middleware_stack()` if the stack is still `None` (`:92-96`). `add_middleware` raises `RuntimeError` once the stack exists (`:104-106`). It inserts at index 0 of `user_middleware`, so the last middleware added becomes the outermost user middleware (`:107`).
- **Splitting handlers (`:68-72`):** the handlers keyed `500` or `Exception` become `error_handler`. All other handlers go into `exception_handlers`.
- **Middleware order (`:74-78`), outermost first:**
  1. `ServerErrorMiddleware(handler=error_handler, debug=debug)`.
  2. `RequestBodyLimitMiddleware`, only if `max_body_size` is set.
  3. `self.user_middleware`.
  4. `ExceptionMiddleware(handlers=exception_handlers, debug=debug)`.
- **Nesting (`:80-83`):** it starts with `app = self.router` and wraps over `reversed(middleware)`, so the first list entry ends up outermost. The result is `ServerError → [BodyLimit] → user… → ExceptionMiddleware → Router`.

## How exceptions reach handlers

- **Registering handlers:** `ExceptionMiddleware.__init__` registers default handlers for `HTTPException` and `WebSocketException` (`middleware/exceptions.py:28-31`). It then adds the user handlers through `add_exception_handler` (`:32-34`). An `int` key goes into `_status_handlers`. A class key goes into `_exception_handlers` (`:41-45`).
- **Per-request setup:** for `http` and `websocket` scopes, `ExceptionMiddleware.__call__` stores both tables in `scope["starlette.exception_handlers"]` (`:52-55`). It then runs `wrap_app_handling_exceptions(self.app, conn)(scope, receive, send)` (`:63`).
- **The wrapper (`_exception_handler.py:23-65`):**
  - It wraps `send` to track whether `http.response.start` has been sent (`:32-39`).
  - It catches `Exception` from the inner app (`:41-43`).
  - For an `HTTPException`, it first looks up the handler by `status_code` (`:46-47`).
  - Otherwise it walks `type(exc).__mro__` and takes the first class with a registered handler (`_lookup_exception_handler`, `:16-20`, called at `:49-50`).
  - If there is no handler, it re-raises (`:52-53`).
  - If the response has already started, it raises `RuntimeError` (`:55-56`).
  - Otherwise it calls the handler. An async handler is awaited, and a sync one runs in a threadpool (`:58-61`). It then sends the returned response (`:62-63`).
- **Routes also use the wrapper:** `routing.py:65` and `:84` call `wrap_app_handling_exceptions` for request and websocket handling inside routes. The handler table comes from the scope, so route-level exceptions are handled the same way, closer to the endpoint.
- **Outermost layer:** `ServerErrorMiddleware` only handles `http` scopes (`middleware/errors.py:150-152`). It catches any `Exception` that got past the inner layers, including exceptions raised by a handler or by user middleware outside `ExceptionMiddleware` (`:165`). It builds the response in one of three ways:
  - In debug mode, it returns an HTML or plain-text traceback (`:167-169`).
  - Otherwise, if a 500 or `Exception` handler is installed, it uses that (`:174-178`).
  - Otherwise it returns plain text "Internal Server Error" (`:171-172`, `:258-259`).

  It sends the response only if one hasn't started (`:180-181`). Then it always re-raises (`:183-186`), so servers can log the error and test clients can surface it.

## Uncertainty

I did not read `Router` or `Mount` internals beyond the two `wrap_app_handling_exceptions` call sites. I also did not read `Middleware.__iter__` (`middleware/__init__.py`), which I assume yields `(cls, args, kwargs)` for the unpacking at `applications.py:81`.