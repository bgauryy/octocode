**Short answer:** `Starlette.build_middleware_stack` builds the stack lazily on the first call. The order is `ServerErrorMiddleware`, then any body-limit middleware, then user middleware, then `ExceptionMiddleware`, then the router. Exceptions propagate outward through that stack. `ExceptionMiddleware` handles the ones that have registered handlers, and `ServerErrorMiddleware` handles the rest as 500s.

## Stack assembly (`starlette/applications.py`)

- **Lazy build:** `__call__` sets `scope["app"] = self`. If `self.middleware_stack is None`, it calls `build_middleware_stack()` and then calls the result (`applications.py:92-96`).
- **Handler split:** `build_middleware_stack` loops over `self.exception_handlers`. A handler keyed `500` or `Exception` becomes `error_handler`. All other handlers go into `exception_handlers` (`:68-72`).
- **Middleware list, outermost first (`:74-78`):**
  1. `ServerErrorMiddleware(handler=error_handler, debug=debug)`.
  2. `RequestBodyLimitMiddleware`, only if `max_body_size` is set.
  3. `self.user_middleware`.
  4. `ExceptionMiddleware(handlers=exception_handlers, debug=debug)`.
- **Wrapping:** it starts with `app = self.router` and iterates `reversed(middleware)`, setting `app = cls(app, *args, **kwargs)` (`:80-83`). The first list entry therefore ends up outermost.
- **User middleware order:** `add_middleware` inserts at index 0 (`:107`), so the most recently added middleware is the outermost of the user middleware. It raises `RuntimeError` if the stack has already been built (`:105-106`).

## How exceptions reach handlers

1. **Registration:** `ExceptionMiddleware.__init__` registers the defaults `HTTPException` → `http_exception` and `WebSocketException` → `websocket_exception`. It then adds the user handlers through `add_exception_handler`. Integer keys go into `_status_handlers` and class keys go into `_exception_handlers` (`middleware/exceptions.py:28-45`).
2. **Per-request setup:** `ExceptionMiddleware.__call__` passes non-http/websocket scopes straight through. For http and websocket scopes it stores both handler dicts in `scope["starlette.exception_handlers"]`. It builds a `Request` or `WebSocket` and runs `wrap_app_handling_exceptions(self.app, conn)(scope, receive, send)` (`exceptions.py:47-63`).
3. **Catching and lookup:** `wrapped_app` in `_exception_handler.py:31-63` wraps `send` to track whether `http.response.start` has gone out. It runs the inner app in a `try`, and on `except Exception` it looks for a handler:
   - For an `HTTPException`, it first checks `status_handlers.get(exc.status_code)` (`:46-47`).
   - Otherwise it calls `_lookup_exception_handler`, which walks `type(exc).__mro__` and returns the first class with a registered handler (`:16-20`, `:49-50`).
   - If no handler is found, it re-raises with `raise exc` (`:52-53`).
   - If a handler is found but the response has already started, it raises `RuntimeError("Caught handled exception, but response already started.")` (`:55-56`).
   - Otherwise it awaits an async handler, or runs a sync one with `run_in_threadpool`, and sends the returned response if it isn't `None` (`:58-63`).
4. **Unhandled exceptions:** An exception with no handler propagates up through the user middleware to `ServerErrorMiddleware` (`middleware/errors.py:149-186`).
   - It only acts on `http` scopes.
   - On an exception it builds a response in one of three ways: a debug traceback response if `debug` is set, the default `error_response` if no handler is installed, or the custom 500/`Exception` handler.
   - It sends that response only if the response hasn't already started (`:180-181`).
   - It then always re-raises (`:186`), so the server can log the error and test clients can see it.

**Where the wrapper is reused:** `wrap_app_handling_exceptions` falls back to empty handlers if the scope key is missing (`_exception_handler.py:26-29`). That fallback implies it is also called from elsewhere, but I didn't open the routing code to confirm, so I haven't verified those call sites.