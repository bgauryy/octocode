**Summary:** PR #16403 was merged on 2026-09-29 (head `daba280d4c82235edc81621ddb869b5bf3879198`). It adds built-in OpenTelemetry traces, metrics and logs to every `FastAPI` app. It is on by default, and it adds a hard dependency on `opentelemetry-api`. I read the full diff via `gh pr diff` and read the new code, but I did not run anything or check out the merged tree. Line numbers below are therefore positions in the diff, not `path:line` at a commit.

## Runtime behavior changes

**1. New public surface**
- `FastAPI(telemetry: TelemetryConfig | None)` is a new keyword argument, stored as `self._telemetry` (`fastapi/applications.py`).
- `TelemetryConfig` keys: `tracer_provider`, `meter_provider`, `logger_provider`, `tracing`, `metrics`, `logs`, `operation_spans`, `auto_configure`, `exclude`. All signals default to on.
- New package `fastapi.telemetry`, which exports `TelemetryConfig`, `TelemetryData` and `get_telemetry_data`.
- `TelemetryData` exposes the request/websocket, body, parsed values and validation errors to synchronous processors. It is meant so that Sentry and Logfire don't have to monkeypatch FastAPI.

**2. A new ASGI entry path in `FastAPI.__call__`**
- Lifespan scopes are wrapped by `telemetry._runtime.lifespan`.
- HTTP and websocket scopes go through `NativeTelemetry` only if `enabled()` is true, which requires at least one signal with a configured, non-proxy provider. Otherwise the old `super().__call__` path runs.
- The request path is also skipped if `"fastapi.telemetry"` is already in the scope, which stops mounted FastAPI apps from double-instrumenting.
- `exclude(scope)` sets `scope["fastapi.telemetry"] = None`. This also hides the request from mounted FastAPI sub-apps.
- `build_middleware_stack` now always inserts `ExceptionTelemetryMiddleware` right after `ServerErrorMiddleware`. It logs exceptions before error handlers turn them into responses.

**3. What gets emitted (`_asgi.py`)**
- A SERVER span is created per request or websocket. It is named `"{METHOD} {route}"`, `WS {route}` or `HTTP` for unknown methods, and it is renamed once the route is known.
- The span takes its parent from W3C headers. A baggage header with multiple values is joined with commas.
- Span attributes include `url.path`, and a `url.query` in which only a fixed allow-list of signature parameters is redacted.
- Metrics: `http.server.request.duration` (histogram with explicit buckets) and `http.server.active_requests`. They are HTTP only, not websocket.
- Logs: WARN for validation failures (error count only, no input). ERROR for unhandled exceptions, including exception type, message and stack trace, via `logger.emit(exception=...)`. Normal websocket close codes 1000 and 1001 are skipped.
- Operation spans (`fastapi.dependencies`, `fastapi.endpoint`, `fastapi.serialization`, `fastapi.background_task`) are added in `routing.py` and `background.py`.

**4. Changes to existing request code (`routing.py`, `background.py`)**
- Sync endpoints now run via `run_in_threadpool(_run_sync_endpoint, function=..., arguments=...)` instead of `run_in_threadpool(dependant.call, **values)`.
- `solve_dependencies` and `serialize_response` are now wrapped in `with _operation(...)`. This reindents those blocks.
- `_route_selected(...)` calls are sprinkled through `_handle_selected`, the frontend handler, and router `app()` (full, partial, redirect and low-priority match paths). They set `http.route` and the span name.
- `BackgroundTasks` now overrides `__call__` to run each task inside a span. It replaces Starlette's implementation and loops over `self.tasks` itself.

**5. Startup side effects (`_runtime.py`)**
- On `lifespan.startup`, `_configure_from_environment` reads `OTEL_EXPORTER_OTLP_*` variables. If an endpoint is set, it adds OTLP HTTP/protobuf exporters to each enabled signal's provider.
- If the global provider is unconfigured, it creates an SDK provider and calls `set_*_provider` on the **process-global** provider.
- Otherwise it registers a new processor or reader on the existing provider.
- Invalid setups raise `FastAPIError` and send `lifespan.startup.failed`. Examples are an exporter other than `otlp` or `none`, a protocol other than `http/protobuf`, a bad URL, or a missing SDK.
- `atexit` shutdown of the owned components, plus a flush in a thread on lifespan shutdown or failure.

**6. Dependencies (`pyproject.toml`)**
- `opentelemetry-api>=1.44.0` becomes a core dependency.
- New `opentelemetry` extra with the SDK and OTLP HTTP exporter. The same two packages are added to `standard`, `standard-no-fastapi-cloud-cli` and `all`.
- The test group gains `opentelemetry-instrumentation-fastapi`, `logfire` and `sentry-sdk`.

## What a reviewer should watch

- **Hard dependency and default-on.** Every FastAPI install now pulls in `opentelemetry-api`, and anyone with an env endpoint plus `standard` now gets automatic export. The `standard` extra now ships the SDK and exporters.
- **Startup can now fail because of OTEL env vars.** For example, `OTEL_TRACES_EXPORTER=console` or `OTEL_EXPORTER_OTLP_PROTOCOL=grpc` makes the app fail at lifespan startup unless `auto_configure=False` is set. Previously those variables did nothing for FastAPI.
- **The app mutates process-global OTEL state.**
  - Two apps, or another library, can race on `set_*_provider`. The code handles that by shutting down its own provider and attaching to the winner.
  - Duplicate export is possible. The code only deduplicates its own registrations (`_configured`), and the docs tell users to avoid configuring one destination twice.
  - `_unconfigured()` and `_registration_provider` match provider classes by module and class name strings. These include the Logfire `ProxyMeterProvider` and private `_ProxyMeterProvider` and `ProxyLoggerProvider` paths. They are fragile against OTEL and Logfire refactors.
- **Interaction with contrib instrumentation.** `_legacy_otel` detects `OpenTelemetryMiddleware` by walking the built middleware stack and turns off native tracing, metrics and logs in that case. `__call__` also builds the middleware stack early if needed. I'd check that this is correct when `instrument_app` is called after the stack is built, and that there is no double counting. Tests in `test_integrations.py` and `test_native_integrations.py` appear to cover this, but I didn't read them.
- **Sensitive data.**
  - Exception messages and tracebacks go to logs by default, even for unsampled traces.
  - `url.query` is exported, and only a short list of parameter names is redacted.
  - `TelemetryData.errors` includes original input values, and `.body` is the raw body. This data isn't exported automatically, but it is exposed to processors.
- **Behavior and hot-path risk in the request path.**
  - The `BackgroundTasks.__call__` override bypasses Starlette's own implementation, so it needs to stay in sync with upstream (for example, the sync-versus-async task handling inside `task()`).
  - The sync endpoint now takes a keyword-wrapper indirection.
  - `_route_selected` is called in many match branches. The redirect branch re-walks `_IncludedRouter._match`, which is the most complicated part to verify.
  - The mount-prefix logic (`_mount_prefix`, `removesuffix("/{path}")`) can produce wrong `http.route` values, so check it with nested mounts and `root_path`.
- **Span and error semantics.** `_traced_operation` marks background task exceptions as ERROR, but HTTP, validation and websocket exceptions are excluded from error status. `except Exception` also does not see `BaseException` such as `CancelledError`.
- **Env var handling.** `OTEL_INSTRUMENTATION_HTTP_KNOWN_METHODS` is read once in the `NativeTelemetry` constructor, at app creation.
- **Size and process.** The PR is about 5.2k added lines and was AI-assisted (per the PR body). The description says Sentry and Logfire confirmed it doesn't block them (2026-09-29 update). The docs warn that independent telemetry configuration for mounted sub-apps is "not guaranteed".

## Uncertainty
- I read the source diff in full except the tail of `NativeTelemetry.__call__` (the `finish()` logic, status codes and the send/receive wrapping) and the lifespan/metrics code beyond what's shown. I did not read the tests beyond their file list.
- I did not run the suite or measure overhead. "Low overhead when disabled" is inferred from the `enabled()` short-circuit, not measured.