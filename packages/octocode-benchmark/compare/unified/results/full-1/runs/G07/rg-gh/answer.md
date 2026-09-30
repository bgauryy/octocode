**Short answer:** `Starlette.build_middleware_stack()` builds the stack lazily and wraps the router in layers. The order, outermost first, is `ServerErrorMiddleware`, then an optional `RequestBodyLimitMiddleware`, then the user middleware, then `ExceptionMiddleware`, then the router. An exception goes to a handler through one of two paths. `ExceptionMiddleware` handles ordinary exceptions. `ServerErrorMiddleware` handles the 500 / `Exception` handler and anything left unhandled.

All paths below are under `starlette/` at 63c5760d8a. I fetched these files with `gh api` and did not read the router or `routing.py`.

## Stack assembly

- **When it's built:** `Starlette.__call__` builds the stack on the first call if `self.middleware_stack is None` (`applications.py:92-96`).
- **Splitting the handlers:** `build_middleware_stack` (`applications.py:63-83`) loops over `self.exception_handlers`.
  - A key of `500` or `Exception` becomes `error_handler`, which goes to `ServerErrorMiddleware` (`:68-72`).
  - All other keys go into `exception_handlers`, which goes to `ExceptionMiddleware`.
- **Layer order:** the `middleware` list is built in this order (`:74-78`):
  1. `ServerErrorMiddleware(handler=error_handler, debug=debug)`
  2. `RequestBodyLimitMiddleware`, only if `max_body_size is not None`
  3. `self.user_middleware`
  4. `ExceptionMiddleware(handlers=exception_handlers, debug=debug)`
- **Wrapping:** the code starts from `app = self.router` and iterates `reversed(middleware)`, setting `app = cls(app, *args, **kwargs)` (`:80-83`). The first list entry therefore ends up outermost.
- **Adding middleware:** `add_middleware` does `user_middleware.insert(0, ...)`, so the most recently added middleware is outermost among the user layers (`:104-107`). It raises `RuntimeError` if the stack already exists.

## Exception routing

**1. `ExceptionMiddleware` (`middleware/exceptions.py`)**
- Its `__init__` registers default handlers for `HTTPException` and `WebSocketException` (`:28-31`). `add_exception_handler` then adds the user handlers (`:32-34`).
- `add_exception_handler` sorts them by key: an int goes into `_status_handlers`, a class goes into `_exception_handlers` (`:41-45`).
- In `__call__`, http and websocket scopes get `scope["starlette.exception_handlers"] = (exception_handlers, status_handlers)` (`:52-55`). It then builds a `Request` or `WebSocket` and calls `wrap_app_handling_exceptions(self.app, conn)` (`:57-63`).
- The default `http_exception` handler returns a bare `Response` for 204 and 304, and otherwise a `PlainTextResponse` of `exc.detail` (`:65-69`).

**2. `wrap_app_handling_exceptions` (`_exception_handler.py:23-65`)**
- It wraps the `send` callable so it can track whether `http.response.start` has been sent (`:31-39`).
- It catches `Exception` from the inner app (`:41-43`).
- **Handler lookup order:**
  1. If the exception is an `HTTPException`, it tries `status_handlers.get(exc.status_code)` (`:46-47`).
  2. Otherwise it calls `_lookup_exception_handler`, which walks `type(exc).__mro__` and returns the first class with a handler (`:16-20`, `:49-50`). A handler for a base class therefore catches subclasses.
  3. If no handler is found, it re-raises (`:52-53`).
- **Running the handler:** if the response has already started, it raises `RuntimeError("Caught handled exception, but response already started.")` (`:55-56`). An async handler is awaited, and a sync handler runs in a threadpool (`:58-61`). A non-`None` response is then sent (`:62-63`).
- **Per-route use:** the same function can be called from elsewhere, since it reads the handlers back from `scope["starlette.exception_handlers"]` (`:27-29`). I didn't check the call sites in `routing.py`.

**3. `ServerErrorMiddleware` (`middleware/errors.py:149-186`)**
- It applies only to `http` scopes (`:150-152`).
- It catches any exception that `ExceptionMiddleware` didn't handle, which includes all unhandled exceptions. It picks a response in this order (`:166-178`):
  - a debug traceback response if `debug` is set
  - `error_response`, the default 500, if there is no custom handler
  - the installed 500 / `Exception` handler otherwise
- It sends the response only if the response hasn't already started (`:180-181`). It then always re-raises the exception (`:183-186`). The code comment says this lets servers log it and lets test clients re-raise it.

## Uncertainty
- I read only `applications.py`, `_exception_handler.py`, `middleware/exceptions.py` and `middleware/errors.py`. I didn't inspect `RequestBodyLimitMiddleware` or the router, so how the router dispatches and raises exceptions is unverified.