**PR #16403 is merged (2026-09-29, merge commit `4b3949cd`). It is a large change: 36 files, and FastAPI core now emits OpenTelemetry data by default.**

I read the PR metadata, the `routing.py` patch, and `_runtime.py` and `pyproject.toml` at the merge commit. I read only parts of `_asgi.py`, and I did not read `_api.py` or `fastapi/background.py` (its patch was cut off). The 21 test files and the other `docs/` files were not opened. I did not run any of the code.

## Runtime behavior changes

1. **A new `telemetry=` argument on `FastAPI()`.**
   - It takes a `TelemetryConfig` dict. The defaults are `tracing`, `metrics`, `logs`, `operation_spans` and `auto_configure` all `True`, plus `exclude=None` and `tracer_provider`, `meter_provider` and `logger_provider` all `None` (`fastapi/applications.py`).
   - The `NativeTelemetry` object is created in `FastAPI.__init__`.

2. **`FastAPI.__call__` is rewritten (`applications.py`).**
   - Lifespan scopes are routed through `fastapi.telemetry._runtime.lifespan`.
   - HTTP and WebSocket scopes go through `NativeTelemetry` only if telemetry is enabled and `"fastapi.telemetry"` is not already in the scope.
   - Otherwise it falls through to `super().__call__`.

3. **Telemetry is enabled automatically only when a provider is configured.** `enabled()` is true if tracing is on and either an explicit provider is passed or the global tracer provider is not "unconfigured" (`_asgi.py:172-184`). The metrics and logs branches are in the omitted lines 185–271, so I did not read them. The docs say "FastAPI provides OpenTelemetry support by default".

4. **What a request does when telemetry is on (`_asgi.py`).**
   - It extracts the parent trace context from the request headers and starts a server span.
   - It records metrics: a duration histogram and an active-requests up/down counter.
   - It attaches the per-request data to the OTel context.
   - It sets `scope["fastapi.telemetry"]` and removes it in a `finally` block.

5. **New middleware in the stack.** `ExceptionTelemetryMiddleware` is inserted right after `ServerErrorMiddleware` (`applications.py`, `build_middleware_stack`). It logs unhandled exceptions as OTel logs, including the stack trace.

6. **Extra spans in `routing.py`.**
   - HTTP requests get spans named `dependencies`, `endpoint` and `serialization`.
   - WebSocket handlers get `dependencies` and `endpoint`.
   - Sync endpoints now run through `_run_sync_endpoint` in the threadpool instead of calling `dependant.call` directly.
   - `BackgroundTasks.__call__` is overridden so each task gets a span (from the docs and the `background.py` diff header).

7. **Route and validation hooks.**
   - `_route_selected(...)` is called at many points in routing: included routers, mounts, the redirect-slash path, and low-priority and frontend routes. It supplies `http.route`.
   - `_validation_failed(...)` is called when request validation fails. The docs say it logs a warning with the route and error count, without the input.
   - `telemetry_data.request`, `.body`, `.values` and `.errors` are stored for third-party integrations to read.

8. **Environment-based export, at lifespan startup (`_runtime.py`).**
   - `_configure_from_environment` runs on `lifespan.startup` and adds OTLP exporters when `OTEL_EXPORTER_OTLP_*ENDPOINT` is set.
   - It is skipped if `OTEL_SDK_DISABLED=true` or `auto_configure` is `False`.
   - It can create and set the global `TracerProvider`, `MeterProvider` or `LoggerProvider`.
   - It flushes owned components when shutdown or startup failure is sent, and registers an `atexit` shutdown.

9. **Dependencies.**
   - `opentelemetry-api>=1.44.0` is now a hard dependency (`pyproject.toml:50`).
   - The SDK and the OTLP HTTP exporter are in the new `opentelemetry` extra and in `standard`, `standard-no-fastapi-cloud-cli` and `all` (`pyproject.toml:61-67, 87-88, 107-108`).
   - The test dependencies add `opentelemetry-instrumentation-fastapi`, `logfire` and `sentry-sdk` (`pyproject.toml:172-174`).

## What a reviewer should watch for

- **Startup can now fail because of environment variables.**
  - `_export_endpoint` raises `FastAPIError` for `OTEL_*_EXPORTER` values other than `otlp` or `none`.
  - It also raises for any protocol other than `http/protobuf`, and for endpoints that are not absolute http(s) URLs.
  - Those errors are turned into `lifespan.startup.failed`. A service that sets standard OTel env vars for another setup, such as gRPC, could fail to start after upgrading, unless it also sets `auto_configure=False`.
  - Missing SDK packages raise too (`_runtime.py:128-131`).

- **Global side effects and possible double export.**
  - FastAPI may install global providers and attach exporters to existing ones.
  - The code explicitly does not deduplicate against other components' exporters (`_runtime.py:86-93`). Using it alongside `opentelemetry-instrumentation-fastapi`, Logfire or Sentry could duplicate data.
  - There is a `_legacy_otel` check for contrib's middleware, but I did not read how its result is used.

- **Sensitive data.**
  - Exception logs include the message and stack trace, and these are emitted even when the trace is not sampled (docs).
  - `url.query` is recorded, and only a short allowlist of parameters is redacted (`_SENSITIVE_QUERY_PARAMETERS` in `_asgi.py`). That list covers AWS, Google and `sig` signatures, but not generic tokens or API keys.
  - `logs` is on by default.

- **Performance on the hot path.**
  - Every operation now uses a `with _operation(...)` wrapper.
  - Sync endpoints go through an extra wrapper.
  - Background tasks are wrapped too.
  - I did not check whether these are cheap when telemetry is disabled or the tracer is a no-op. The `NativeTelemetry` shortcut covers this at the whole-app level, but `_operation` itself needs checking.

- **Routing churn.** `_route_selected` calls were added at roughly seven sites in `routing.py`. The redirect branch re-walks `_IncludedRouter._match`, and the risks are wrong or missing `http.route` values, or extra matching work.

- **Core code now depends on `opentelemetry-api` at import time.** The new modules import `opentelemetry` directly (`_asgi.py:17-22`). This is why the dependency is a hard one.

- **Known limitations the docs state.**
  - Mounted sub-applications don't get independent telemetry configuration.
  - Metrics cover HTTP only, not WebSocket.
  - Background task spans end after the response span.

- **Process notes.**
  - The PR body says it was written with AI help and manually reviewed.
  - A comment dated 2026-09-29 says Sentry and Logfire confirmed it doesn't block them.
  - The PR author expects to refactor these internals soon.

## Not verified

I did not read the files below. I also did not read the omitted regions of `_asgi.py` (lines 31–144, 185–271, 297–321 and 348–430), so `finish()`, metric and attribute naming, and the exclude handling are unchecked.

- `fastapi/telemetry/_api.py`
- `fastapi/telemetry/__init__.py`
- the rest of `fastapi/background.py`
- the 21 test files