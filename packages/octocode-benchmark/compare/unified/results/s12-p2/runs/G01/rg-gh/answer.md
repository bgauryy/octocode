PR #16403 is merged. tiangolo opened it, and the head commit is `daba280d`. It adds 5,251 lines and removes 46 across 36 files, most of it tests and docs. I read the diffs for `applications.py`, `background.py`, `routing.py`, `pyproject.toml`, `telemetry/_runtime.py` and the docs page. For `_asgi.py` I only read `_legacy_otel` and `NativeTelemetry.enabled`. I did not read `_api.py` or the tests, and I did not run anything.

## Runtime behavior changes

**1. New `telemetry=` argument on `FastAPI()`** (`fastapi/applications.py`).
- It takes a `TelemetryConfig` dict with these keys:
  - the three providers (`tracer_provider`, `meter_provider`, `logger_provider`);
  - `tracing`, `metrics` and `logs`;
  - `operation_spans`;
  - `auto_configure`;
  - `exclude`.
- Every signal is on by default and the providers default to `None`. `None` means the OpenTelemetry global provider is used.

**2. `FastAPI.__call__` is rewritten** (`applications.py`, in the `__call__` hunk).
- Lifespan scopes now go through `fastapi.telemetry._runtime.lifespan`.
- HTTP and websocket scopes go through `NativeTelemetry` only if `enabled()` is true. They skip it when `"fastapi.telemetry"` is already in the scope.
- `enabled()` (`_asgi.py:172`) is true only if some signal is on and its provider is explicit or non-default. With no OpenTelemetry SDK configured, requests take the old path.
- When telemetry is active, `__call__` builds `middleware_stack` eagerly and sets `scope["app"]` itself.

**3. A new `ExceptionTelemetryMiddleware`** sits just inside `ServerErrorMiddleware` in `build_middleware_stack`. It is always added, even when telemetry is off.

**4. Auto-configuration from environment variables** (`_runtime.py`).
- On `lifespan.startup`, `_configure_from_environment` runs. If `OTEL_EXPORTER_OTLP_*ENDPOINT` is set, it adds OTLP HTTP exporters to the selected providers.
- If no SDK provider is installed, it creates one and registers it globally.
- It also registers an `atexit` hook and flushes on lifespan shutdown.
- It raises `FastAPIError` and sends `lifespan.startup.failed` in these cases:
  - the exporter is neither `otlp` nor `none`;
  - the protocol is not `http/protobuf`;
  - the endpoint URL is invalid;
  - the SDK extras are missing;
  - the provider has no way to add an exporter.
- Setting `OTEL_SDK_DISABLED=true` or `auto_configure=False` turns this off.

**5. New spans in the request path** (`routing.py`).
- `_operation(...)` wraps `dependencies`, `endpoint`, `serialization` and `background_task` spans. The background task span comes from a new `BackgroundTasks.__call__` override in `background.py`.
- Sync endpoints now run through `_run_sync_endpoint` inside `run_in_threadpool`.
- `_route_selected(...)` is called in all the routing and mount match paths. It records the route template for the span name and for `http.route`.
- `_validation_failed(...)` is called when validation fails.
- The handler stashes `request`, `body`, `values` and `errors` into per-request telemetry data. This is the data the PR description says Logfire and Sentry currently monkeypatch FastAPI to get.

**6. Dependencies** (`pyproject.toml`).
- `opentelemetry-api>=1.44.0` becomes a hard dependency of every install.
- The SDK and OTLP HTTP exporter go into a new `opentelemetry` extra. They are also added to `standard`, `standard-no-fastapi-cloud-cli` and `all`.

## What a reviewer should watch

- **Double instrumentation.**
  - `_legacy_otel` (`_asgi.py:72`) detects contrib's `OpenTelemetryMiddleware` by matching its module and class name in the built stack. When it finds one, native tracing, logs and metrics are switched off.
  - If someone calls `instrument_app` after the stack is built, or wraps the app externally, you could get duplicate spans or metrics. Check that the tests cover both orders.
- **Telemetry now turns on for `fastapi[standard]` users.**
  - The SDK is bundled, so a user who already sets `OTEL_EXPORTER_OTLP_ENDPOINT` for another library gets a second exporter added. The docs and code say FastAPI does not deduplicate.
  - Startup can now fail on settings that used to be harmless, such as `OTEL_TRACES_EXPORTER=console` or the gRPC protocol.
  - I couldn't tell from what I read whether the exporter or protocol check applies only to signals that are enabled. The `requested` list is filtered by `enabled`, so probably it does.
- **Global state and thread-safety.**
  - `_runtime.py` uses module-level `_owned` and `_configured` lists, an RLock and `atexit`.
  - `_registration_provider` special-cases Logfire's `ProxyMeterProvider` by class name.
  - There is also a race-handling path when another component sets the global provider first. Reviewers should check these and the shutdown ordering.
  - Multiple apps in one process and mounted sub-apps are explicitly not guaranteed to be independent. The docs warn about this.
- **Hot-path cost and subtle behavior changes in `routing.py`.**
  - Each request now enters `_operation` context managers and per-request data capture. Ask for a benchmark with telemetry both off and on.
  - Sync endpoints go through a new wrapper in the threadpool. Check that context propagation, exception types and tracebacks are unchanged.
  - `BackgroundTasks.__call__` is overridden, so any subclass or user override of it behaves differently.
  - `ExceptionTelemetryMiddleware` is always in the stack, so it changes the middleware list that user code or tests may introspect.
- **Data exposure.** The handler now keeps the request body and the solved dependency values in telemetry data. I did not check which of these end up in span attributes or logs, or whether anything is redacted. That needs a look in `_api.py` and `_asgi.py`.
- **Eager `build_middleware_stack` in `__call__`.** It is now built on the first request instead of lazily by Starlette. Check that this has no ordering side effects, for example when middleware is added after startup.
- **Docs.** I did not fully review them. The docs say FastAPI Cloud works with no configuration, which the PR does not demonstrate.
- **Test dependencies.** The test dependency group adds `logfire`, `sentry-sdk` and `opentelemetry-instrumentation-fastapi`. Coverage is also changed to `patch = ["subprocess", "_exit"]`. Expect CI time and dependency weight to go up.

## Uncertainty

I only read part of `_asgi.py` and none of `_api.py`, so the span attributes, semantic-convention details, the exception and error handling, and the `exclude` behavior are unverified. The PR author notes that Sentry and Logfire were checked on 2026-09-29. I did not confirm that independently.