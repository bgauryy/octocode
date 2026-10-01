The stack is built lazily on the first ASGI call. It is `ServerErrorMiddleware`, then any body-limit middleware, then your middleware, then `ExceptionMiddleware`, then the router. Raised exceptions go to `ExceptionMiddleware` first, which dispatches to handlers registered for status codes or exception classes. Anything not handled there falls through to `ServerErrorMiddleware`. All of this is at commit 63c5760d8a.

**Assembly**

- `Starlette.__init__` only stores the pieces: `self.router`, `self.exception_handlers`, `self.user_middleware` and `self.middleware_stack = None` (`starlette/applications.py`, in `__init__`).
- `Starlette.__call__` sets `scope["app"]`. If `middleware_stack` is still `None`, it calls `build_middleware_stack()`, then awaits the stack. I read these in the fetched excerpt, but it gave no line numbers for them. `build_middleware_stack` itself starts at roughly line 63.
- `build_middleware_stack` splits `self.exception_handlers`. Handlers keyed `500` or `Exception` become `error_handler`. Everything else goes to the `exception_handlers` dict.
- It then builds the middleware list in this order:
  1. `ServerErrorMiddleware(handler=error_handler, debug=debug)`, outermost.
  2. `RequestBodyLimitMiddleware`, only if `max_body_size` is not `None`.
  3. `*self.user_middleware`, in the order given.
  4. `ExceptionMiddleware(handlers=exception_handlers, debug=debug)`, innermost.
- It starts with `app = self.router` and wraps in `reversed(middleware)`, so the first list entry ends up outermost.

**Exception flow**

1. `ExceptionMiddleware.__call__` (`starlette/middleware/exceptions.py`) passes non-`http`/`websocket` scopes straight through.
2. For `http` and `websocket` scopes it stores `(self._exception_handlers, self._status_handlers)` in `scope["starlette.exception_handlers"]`.
3. It builds a `Request` or `WebSocket` and runs `wrap_app_handling_exceptions(self.app, conn)(scope, receive, send)`.
4. `ExceptionMiddleware` registers defaults for `HTTPException` (`http_exception`) and `WebSocketException` (`websocket_exception`). Integer keys go into the status-handler dict, and class keys go into the exception-handler dict.
5. `wrap_app_handling_exceptions` (`starlette/_exception_handler.py`) wraps the inner app in `try/except Exception`. It wraps `send` to track whether `http.response.start` has been sent.
6. Handler lookup in `wrap_app_handling_exceptions`:
   - An `HTTPException` first tries `status_handlers.get(exc.status_code)`.
   - Otherwise `_lookup_exception_handler` walks `type(exc).__mro__`, so a handler for a base class also catches subclasses.
   - If no handler is found, it re-raises with `raise exc`.
   - If a handler is found but the response has already started, it raises `RuntimeError("Caught handled exception, but response already started.")`.
   - Otherwise it calls the handler. An async handler is awaited, and a sync handler runs via `run_in_threadpool`. The handler's response is then sent.
7. Unhandled exceptions, including `Exception` and `500` handlers, reach `ServerErrorMiddleware.__call__` (`starlette/middleware/errors.py`). It only acts on `http` scopes. It picks a response as follows:
   - In debug mode, a traceback response.
   - Otherwise the installed handler, if any.
   - Otherwise the default `error_response`.
   - It sends that response only if the response hasn't started, then always re-raises `exc` so servers or test clients can see it.

**Not verified**

- My search for other callers of `wrap_app_handling_exceptions` failed on a validation error and I didn't retry.
- I did not read the router or `Route` code, so I can't say whether routes also call `wrap_app_handling_exceptions`. The `KeyError` fallback in `wrap_app_handling_exceptions` suggests it is meant to be callable outside `ExceptionMiddleware`.
- I did not read `RequestBodyLimitMiddleware`.