I couldn't verify this at commit `63c5760d8a`. No Octocode tool schemas were available in this session, so I made no tool calls and have no `path:line` evidence. What follows is from my general knowledge of Starlette and may differ at that commit. Treat it as unverified.

**Stack assembly** (`Starlette.build_middleware_stack` in `starlette/applications.py`)
- The stack is built lazily, on the first `__call__`, when `self.middleware_stack is None`.
- Handlers registered for `500` or `Exception` go to `ServerErrorMiddleware`. All other handlers go to `ExceptionMiddleware`.
- The resulting order, outermost to innermost, is:
  1. `ServerErrorMiddleware`
  2. the user middleware, in the order given
  3. `ExceptionMiddleware`
  4. `self.router`
- It wraps the app by iterating `reversed(middleware)` and doing `app = cls(app, *args, **kwargs)`.

**Exception routing**
- `ExceptionMiddleware` (`starlette/middleware/exceptions.py`) uses `wrap_app_handling_exceptions` from `starlette/_exception_handler.py`.
- It looks up a handler first by the status code for an `HTTPException`, then by exception class along the MRO.
- Routes and the router also wrap their endpoints with the same helper, so handlers apply close to where the exception is raised.
- `ServerErrorMiddleware` is the outermost layer and catches anything unhandled. It calls the `500`/`Exception` handler, or renders the debug traceback page when debug is on. It then re-raises the exception so the server can log it.

To get this verified with line numbers, re-run the session with the Octocode tools exposed.