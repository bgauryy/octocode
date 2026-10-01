PR #16403 was merged on 2026-09-29 or later. Its merge commit is `4b3949cd9e5932f76217131fceabc74145c3db24`, and it changes 36 files (+5251/−46). It adds built-in OpenTelemetry tracing, metrics and logs to `FastAPI`. The code is on by default: it only produces data once a real OTel provider exists, or once OTLP env vars are set with the SDK installed. I read the diffs for `applications.py`, `routing.py`, `background.py` and the start of `pyproject.toml`, plus `_runtime.py` in full and `_asgi.py` lines 72–200. I did not read `_api.py`, the rest of `_asgi.py`, the tests or the docs, so I can't vouch for span or metric naming, attribute content, or how well the tests cover it.

## Runtime changes

**New API and config**
- `FastAPI(telemetry=TelemetryConfig)` is a new constructor argument. `applications.py` merges it over these defaults:
  - `tracer_provider`, `meter_provider`, `logger_provider`, `exclude`: `None`
  - `tracing`, `metrics`, `logs`, `operation_spans`, `auto_configure`: `True`
- New package `fastapi/telemetry/` holds `_api.py`, `_asgi.py` and `_runtime.py`.

**Per-request path** (`applications.py`, `FastAPI.__call__`)
- Lifespan scopes go through `telemetry._runtime.lifespan`.
- HTTP and websocket scopes go through `NativeTelemetry` only if `self._native_telemetry.enabled()` is true and the scope isn't already marked `"fastapi.telemetry"`.
- `enabled()` (`_asgi.py:172`) is true only if some signal is on and either an explicit provider is set or the global provider isn't the no-op one (`_unconfigured`). With no OTel setup, requests take the old path.
- `__call__` now builds `middleware_stack` and sets `scope["app"]` itself when telemetry is enabled, so some of Starlette's setup is duplicated.

**Middleware stack**
- `ExceptionTelemetryMiddleware` is added right after `ServerErrorMiddleware`, so every app gets an extra layer. It records exceptions before handlers turn them into responses, and emits an error log record when a logger is present (`_asgi.py:87–143`).

**Routing** (`routing.py`)
- `_route_selected(...)` is called at about 6 match sites: normal match, partial match, redirect-slash, low-priority match, and the included-router and mount paths. This sets the route template for `http.route`.
- The redirect branch re-runs `_match` on included routers, but only when telemetry is active.
- Operation spans wrap:
  - endpoint
  - dependencies
  - serialization
  - `background_task`

  Sync endpoints now run through `_run_sync_endpoint` inside `run_in_threadpool`.
- The request handler stores `request`, `body`, `values` and `errors` on a telemetry data object (`get_telemetry_data()`), and `_validation_failed` is called before validation errors are raised.
- `BackgroundTasks.__call__` is overridden to wrap each task in a span.

**Startup and shutdown** (`_runtime.py`)
- With `auto_configure`, on `lifespan.startup` it reads the `OTEL_*` env vars and adds OTLP exporters:
  - Only `OTEL_<SIGNAL>_EXPORTER` set to `otlp` or `none`, and the `http/protobuf` protocol, are supported. Anything else raises `FastAPIError`.
  - It creates SDK providers if the global one is unconfigured.
  - Otherwise it attaches a processor or reader to the existing provider.
  - It has a workaround for old Logfire wrappers.
- A failure during that setup sends `lifespan.startup.failed` and re-raises.
- It flushes on lifespan shutdown and registers an `atexit` shutdown.

**Dependencies** (`pyproject.toml`)
- `opentelemetry-api>=1.44.0` becomes a hard dependency of fastapi.
- The SDK and OTLP HTTP exporter go into a new `opentelemetry` extra and into `standard`.

## What to watch

1. **New hard dependency.** Every install now pulls `opentelemetry-api`. Check the lower bound of 1.44.0 and the compatibility story.
2. **Double instrumentation.** `_legacy_otel` (`_asgi.py:72`) walks the built middleware stack looking for contrib's `OpenTelemetryMiddleware` by module and class name. If a user has contrib instrumentation, the native code changes behavior, but I didn't read how. The name-based detection is brittle, so confirm the tests cover both orderings: contrib applied before or after stack creation.
3. **Hot path on every request.** `enabled()` runs on every request and calls the global getters. Spans wrap dependencies and serialization, and the handler now stores the request body on a data object. Check the overhead when tracing is on. Check also that the `request.body` retention doesn't hold large payloads or leak into attributes, and that sensitive data is handled. `_SENSITIVE_QUERY_PARAMETERS` is a small fixed list of query-parameter names.
4. **Copied `__call__` logic.** The `middleware_stack is None` and `scope["app"]` handling is copied from Starlette. Errors in it could differ between the telemetry and non-telemetry paths. The `routing.py` redirect branch also replicates matching logic.
5. **Startup failures become fatal.** Bad `OTEL_*` values raise `FastAPIError` and fail app startup. That happens even when the user never asked for FastAPI's telemetry, because `auto_configure` defaults to `True`. Only `OTEL_SDK_DISABLED=true` or `auto_configure=False` skips it. Two smaller points:
   - It mutates global providers with `set_*_provider`. There is a race-handling path, but it is hard to test.
   - `atexit` and a thread-pool flush run at shutdown.
6. **Sync endpoints.** They now go through `_run_sync_endpoint`. Check that contextvars and trace context propagate into the thread, and that there is no behavior change for `run_in_threadpool`.
7. **Third-party patching.** The PR description says the design accommodates Logfire and Sentry, which would otherwise monkeypatch FastAPI. The description also says the PR author checked with both on 2026-09-29. The author says they plan to refactor these internals soon, so the private `_route_selected`/`_operation` hooks are not stable API.
8. **Size and AI use.** About 5k lines were added, of which roughly 3.6k is tests. The description says it was made with AI and manually reviewed. Focus on the asgi/runtime edge cases and the websocket paths.

## Uncertainty
- I have not seen the full diff for `_api.py` and most of `_asgi.py`, including the `__call__` body and the metric and span names. I also did not check the tests or the docs.
- I assumed behavior is unchanged when no provider is configured. That follows from `enabled()` and the `__call__` branch, but I haven't run it.