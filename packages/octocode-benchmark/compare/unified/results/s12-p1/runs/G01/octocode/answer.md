PR #16403 is merged (merge commit `4b3949c`, 2026-09-29). It adds built-in OpenTelemetry tracing, metrics and logs to every `FastAPI` app, on by default. I read the patches for `applications.py`, `routing.py` and `background.py` and the full `fastapi/telemetry/_runtime.py`. I did not read `_asgi.py` (446 lines), `_api.py` (276 lines), the tests, or the end of the `pyproject.toml` patch.

## Runtime behavior changes

**New constructor option.** `FastAPI(telemetry=...)` takes a `TelemetryConfig` dict. The defaults, from the `applications.py` patch at about line 1030, are:
- `tracing`, `metrics`, `logs`, `operation_spans` and `auto_configure` all `True`.
- `tracer_provider`, `meter_provider`, `logger_provider` and `exclude` all `None`, so the global providers are used.

**Request path (`applications.py` `__call__`).**
- Lifespan scopes always go through `fastapi.telemetry._runtime.lifespan`.
- HTTP and websocket scopes are wrapped in `NativeTelemetry` unless `"fastapi.telemetry"` is already in the scope or `_native_telemetry.enabled()` is false.
- `self.middleware_stack` is now built eagerly there, and `_legacy_otel(self.middleware_stack)` is passed in. Presumably this detects the contrib instrumentation to avoid double-instrumenting, but I didn't read it.
- `build_middleware_stack` now inserts `ExceptionTelemetryMiddleware` right after `ServerErrorMiddleware`.

**Instrumentation in `routing.py` and `background.py`.**
- `_operation(...)` spans now wrap these steps: the endpoint (`name="endpoint"`), dependency solving (`"dependencies"`), response serialization (`"serialization"`) and each background task (`"background_task"`). This applies to the websocket handler as well.
- Sync endpoints now run via `_run_sync_endpoint` in the threadpool instead of calling `run_in_threadpool(dependant.call, ...)` directly.
- `BackgroundTasks.__call__` is overridden to wrap each task in a span.
- `_route_selected` is called at every route-selection point, so the route template can be recorded.
- `_validation_failed` is called before the validation errors are raised.
- A per-request telemetry data object receives `request`, `body`, `values`, `errors` and `websocket`. The PR description says this exists so Logfire and Sentry don't have to monkeypatch FastAPI.

**Environment-based export (`_runtime.py`).**
- `_configure_from_environment` runs on `lifespan.startup`. It reads the `OTEL_*` variables and the `OTEL_EXPORTER_OTLP_*` endpoint and protocol variables.
- It adds OTLP/HTTP exporters to the selected providers.
- If the global provider is unconfigured, it creates an SDK provider and calls `set_tracer_provider`, `set_meter_provider` or `set_logger_provider` globally (`_runtime.py:157-171`).
- It registers an `atexit` shutdown (`:39`) and force-flushes on lifespan shutdown (`:229-236`).
- It does nothing when `OTEL_SDK_DISABLED=true` or `auto_configure=False`.

**Dependencies (`pyproject.toml`).**
- `opentelemetry-api>=1.44.0` becomes a hard dependency.
- `opentelemetry-sdk` and `opentelemetry-exporter-otlp-proto-http` are added to a new `opentelemetry` extra and, from the visible patch, to `standard`. I didn't see the end of the patch, so confirm that.

## What a reviewer should watch

1. **Default-on behavior change.** Every app gets spans, metrics and logs automatically. A new hard dependency on `opentelemetry-api` also affects all installs. Measure the per-request overhead when no SDK is configured, since the API should then be a no-op.
2. **Hot-path edits to `routing.py`.** `_route_selected` is called in about six places, including mounts, the redirect-slash path and the low-priority match. The redirect branch re-walks `_IncludedRouter._match` in a `while` loop (`routing.py` around 2806-2820). That is complex and is only exercised when telemetry data exists in the scope.
3. **Global side effects.**
   - Startup can set global OpenTelemetry providers and register `atexit` hooks.
   - It can attach exporters to providers that other libraries own. `_runtime.py:177-180` handles a race by shutting down its own provider and attaching to the other one.
   - There is special-casing for Logfire's `ProxyMeterProvider` (`:73-82`).
   - The module uses module-level mutable state (`_owned`, `_configured`).
4. **Startup can now fail.**
   - Unsupported `OTEL_*_EXPORTER` values, a non-`http/protobuf` protocol, a malformed endpoint, or a missing SDK extra raise `FastAPIError`.
   - That is surfaced as `lifespan.startup.failed` (`:219-223`).
   - So an environment that previously started fine, such as one with `OTEL_EXPORTER_OTLP_PROTOCOL=grpc`, could now fail to boot. Check this against the documented opt-out.
5. **Lifespan handling.** The app now always wraps lifespan, replacing `super().__call__` with a wrapper. Check this against servers and tests that rely on lifespan being unsupported, and against user-provided lifespans.
6. **Data retention.** The request body, solved values and errors are stored in the telemetry data object. Check for memory use, PII and sensitive-data exposure in span attributes, and whether `exclude` is enough to control this.
7. **Behavior change in `BackgroundTasks`.** `__call__` is overridden, so any subclass or caller relying on Starlette's implementation is affected.
8. **Size and provenance.** The PR is +5251/−46 across 36 files and was Codex-assisted (the PR body says "manually reviewed"). About 3,000 lines are tests, and the new runtime code in `fastapi/telemetry/` is about 960 lines. The author reports checking with Sentry and Logfire, and a follow-up with OpenTelemetry contrib is suggested.

**Not verified:** span names and attributes, the semantic-convention details, the `exclude` semantics, the `_legacy_otel` detection logic, and the test coverage in `_asgi.py` and `_api.py`.