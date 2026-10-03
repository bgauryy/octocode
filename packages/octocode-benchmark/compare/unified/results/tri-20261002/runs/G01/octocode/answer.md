PR #16403 is merged (merge commit `4b3949cd`). It adds built-in OpenTelemetry tracing, metrics and logs to every `FastAPI` app. It is active whenever an OTel provider is configured. I read the diffs for `applications.py`, `routing.py` and `background.py`, and the merged `_runtime.py` and `_asgi.py` (all but the last ~9 lines of `_asgi.py`). I did not read `_api.py`, the `pyproject.toml` diff, the tests or the docs.

## Runtime changes

**New `telemetry=` option on `FastAPI()`** (`applications.py`)
- It takes a `TelemetryConfig` dict. The defaults are:
  - `tracing`, `metrics`, `logs`, `operation_spans` and `auto_configure` are all `True`.
  - The three providers are `None` (use the global ones).
  - `exclude` is `None`.

**`FastAPI.__call__` now branches by scope type** (`applications.py`)
- `lifespan` scopes go through `telemetry._runtime.lifespan`.
- `http` and `websocket` scopes go through `NativeTelemetry`.
- The fast path is unchanged: other scope types, scopes that already carry `"fastapi.telemetry"` (nested apps), and apps where `enabled()` is false call `super().__call__` directly.

**It is not an opt-in feature.**
- `enabled()` is true if any signal is on and has an explicit or non-default global provider (`_asgi.py:172-196`). Merely having an SDK provider installed switches it on.

**What it emits** (`_asgi.py`)
- It starts a SERVER span per request or websocket and extracts the parent from the incoming headers (`_asgi.py:282-316`).
- It records `http.server.request.duration` and `http.server.active_requests`. Metrics are skipped for websockets (`_asgi.py:244`).
- It emits an exception log record for unhandled errors.
- `url.query` values are redacted only for five parameter names (`_asgi.py:44-52`, `295-301`).

**Extra work in the request path** (`routing.py`, `background.py`)
- Child "operation" spans wrap the endpoint, dependencies, serialization and each background task (`routing.py`, `background.py`).
- Sync endpoints now run through a `_run_sync_endpoint` wrapper instead of calling `run_in_threadpool` on the endpoint directly.
- The request handler stashes `request`, `body`, solved `values` and `errors` on a context object, so they are held for the life of the request.
- Validation failures go through `_validation_failed(...)`.
- `_route_selected(...)` is called at about six routing points, including the redirect and low-priority-match paths, to set `http.route`. One of them re-walks `_IncludedRouter._match` for redirects.
- `BackgroundTasks.__call__` is overridden, so it no longer delegates to Starlette's implementation.

**Middleware stack** (`applications.py`)
- An `ExceptionTelemetryMiddleware` is now always inserted right after `ServerErrorMiddleware`. It sees exceptions before the error handlers turn them into responses.

**Lifespan auto-configure** (`_runtime.py`)
- On `lifespan.startup`, if `OTEL_*_ENDPOINT` env vars are set, FastAPI creates OTLP http/protobuf exporters.
  - If the global provider is unconfigured, it installs a new SDK provider as the global one.
  - Otherwise it adds a processor or reader to the existing provider.
- Unsupported settings (a non-otlp exporter, a non-http/protobuf protocol, a bad URL, or the missing extra) raise `FastAPIError`. The lifespan wrapper then sends `lifespan.startup.failed`, so the app fails to start (`_runtime.py:42-70`, `128-131`, `216-224`).
- On shutdown it flushes and shuts down the providers it owns. It also registers an `atexit` hook.

**Coexistence with the contrib instrumentation**
- If the built middleware stack contains contrib's `OpenTelemetryMiddleware`, native tracing, metrics and logs are suppressed (`_legacy_otel`, `_asgi.py:72-84`, `235-244`). The detection depends on module and class name strings.

## What a reviewer should watch out for

1. **Behavior change for existing users.** An app that already has OTel configured, for example through contrib auto-instrumentation or Logfire, gets native telemetry without opting in. Check that the double-instrumentation guard is reliable:
   - It only checks the stack as it exists at request time.
   - It relies on the name-string match above.
   - Apps mounted inside other apps depend on the `scope["fastapi.telemetry"]` marker, which the `exclude` path sets to `None` (`_asgi.py:226-234`).
2. **Startup failure from environment variables.** Setting `OTEL_EXPORTER_OTLP_*` (for example grpc, or a console exporter) now makes startup fail unless `auto_configure=False`. Global state is also mutated at startup: it sets global providers and registers `atexit`. It is guarded by a module-level lock and a `_configured` list, so multiple apps and tests could interact.
3. **Hot-path cost and memory.** Every request now has extra context attach/detach, wrapper callbacks, a `parse_qsl`/`urlencode` rebuild of the query string, and per-operation spans. Operation spans can be turned off with `operation_spans=False`. The `body` and `values` references stay alive for the whole request.
4. **Edits inside core routing.** `routing.py` is `+108/−32` and touches `get_request_handler`, the websocket app, and the redirect, low-priority and included-router matching paths. Look for changed exception semantics: `WebSocketRequestValidationError` is now built into a variable before being raised, and `BackgroundTasks.__call__` was reimplemented.
5. **Privacy and cardinality.**
   - Only a short fixed list of query parameter names is redacted. Other secrets in the query string go into `url.path` and `url.query` span attributes.
   - Check how `http.route` behaves for mounts and for unmatched 404s.
   - Check the HTTP method normalization via `OTEL_INSTRUMENTATION_HTTP_KNOWN_METHODS`.
6. **Dependency and packaging.** `pyproject.toml` has 18 added lines and `uv.lock` changed by +96/−12. The runtime modules import `opentelemetry` at module level (`_asgi.py:17-22`, `_runtime.py:12`), and `applications.py` imports `fastapi.telemetry._asgi` at top level. I did not confirm that `opentelemetry-api` is a hard dependency rather than behind an extra. If it is only in an extra, a bare `pip install fastapi` could fail on import, so check this first.
7. **Size and process.** The PR is 36 files and +5251 lines, about 3,000 of them tests. The author says it was made with AI and manually reviewed. The author also says Sentry and Logfire confirmed it doesn't block them, and that a follow-up with contrib is planned.

## Not verified
- I did not read `_api.py` (the operation, `_run_sync_endpoint` and `_unconfigured` helpers), the last ~9 lines of `_asgi.py`, the `pyproject.toml` diff, the tests or the docs.
- I did not check the later history of `master` for follow-up fixes.