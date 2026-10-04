**Summary:** PR #16403 is merged (head `daba280d`). It adds built-in OpenTelemetry to FastAPI, and the feature is on by default. The change is +5251/−46 across 36 files. I read the diffs for `applications.py`, `background.py`, `routing.py` and `pyproject.toml`, and the full `_api.py` and `_runtime.py`. I only skimmed `_asgi.py` (about 446 lines, the span and metric logic), using a grep for definitions. I did not read the tests or the docs.

## Runtime behavior changes

- **New constructor argument.** `FastAPI(telemetry=TelemetryConfig)` sets defaults of tracing, metrics, logs, `operation_spans` and `auto_configure` all `True` (`applications.py`, hunk at ~1030). The `exclude` callback defaults to `None`.
- **`__call__` now branches on scope type.**
  - Lifespan scopes go through `fastapi.telemetry._runtime.lifespan`.
  - HTTP and WebSocket scopes go through `NativeTelemetry` unless telemetry is disabled or `"fastapi.telemetry"` is already in the scope.
  - In that path FastAPI builds the middleware stack early and sets `scope["app"]` itself.
- **New middleware.** `ExceptionTelemetryMiddleware` is inserted right after `ServerErrorMiddleware` in `build_middleware_stack`, so it sits outside all user middleware.
- **New spans.** A server span is created per request or WebSocket connection. `_operation` adds child spans named `fastapi.dependencies`, `fastapi.endpoint`, `fastapi.serialization` and `fastapi.background_task` (`routing.py`, `background.py`, `_api.py:186-233`).
  - Operation spans record `error.type` and set ERROR status, except for HTTP and validation exceptions and normal WebSocket disconnects. Background-task spans always record the error.
  - Background-task spans start after the server span ends (the test asserts `span.start_time >= server.end_time`).
- **Route naming.** `_route_selected` is called at many routing points: included routers, mounts, redirects and low-priority matches. It sets `http.route` and renames the span to `METHOD route`, tracking a mount prefix (`_api.py:243-260`).
- **Sync endpoints.** They now run through `_run_sync_endpoint` in the threadpool, so the span is created inside the worker thread (`routing.py`, `run_endpoint_function`).
- **`BackgroundTasks.__call__` is overridden** to wrap each task in a span (`background.py`).
- **Validation failures** emit a WARN log event `fastapi.validation.failed` (`_api.py:147-170`).
- **Metrics.** HTTP metrics are skipped for WebSockets (`_asgi.py:244`).
- **Public data hook.** `get_telemetry_data()` exposes the request, body, parsed values and validation errors to synchronous processors (`_api.py:135`). The docs say these are not exported automatically.
- **Env-based auto-export** (`_runtime.py`).
  - At lifespan startup, `_configure_from_environment` adds OTLP exporters when `OTEL_EXPORTER_OTLP_*ENDPOINT` is set. It creates and installs global providers only if the current ones are the API's unconfigured proxy providers.
  - It supports only `OTEL_*_EXPORTER=otlp` or `none`, and only the `http/protobuf` protocol. Anything else raises `FastAPIError`, which is reported as `lifespan.startup.failed`.
  - On shutdown it flushes, and an `atexit` hook shuts down providers FastAPI created.
- **Dependencies.** `opentelemetry-api>=1.44.0` is now a hard dependency. The SDK and OTLP HTTP exporter are added to the `opentelemetry`, `standard`, `standard-no-fastapi-cloud-cli` and `all` extras (`pyproject.toml`).

## Reviewer watch-outs

1. **Hard dependency and default-on.** Every FastAPI install now pulls in `opentelemetry-api`. Overhead when nothing is configured depends on `_asgi.py`, which I didn't read. `_operation` returns a shared `nullcontext` when there is no request telemetry or tracer (`_api.py:189-191`), which is cheap. Check that `NativeTelemetry.enabled()` short-circuits properly.
2. **Startup side effects.** An environment variable alone can now install global providers and make network exporters. Failures abort startup instead of degrading quietly. The `OTEL_EXPORTER` and protocol restrictions are strict, so someone using `grpc` gets a startup error unless they set `auto_configure=False`.
3. **Duplicate instrumentation.** `_legacy_otel(self.middleware_stack)` detects the contrib `OpenTelemetryMiddleware` by class name and then turns off native tracing, logs and metrics (`_asgi.py:72-80`, `235-244`). Name-based detection is fragile. It also means `FastAPI(telemetry=...)` is silently ignored in that case. Logfire and Sentry are handled by duck-typing, such as the `ProxyMeterProvider` workaround in `_runtime.py:73-82`. That is vendor-specific code inside core.
4. **Sensitive data.** The `logs` option records exception messages and stack traces (`_api.py:60-65`). `TelemetryData` exposes request bodies and validation errors that include the original input values (`_api.py:107-110`). The docs tell integrations to redact, but FastAPI doesn't.
5. **Behavior changes to existing paths.**
   - Dependency resolution is now wrapped in a `with`, and the `telemetry_data.values/errors` assignments are inside that block.
   - Middleware order is changed.
   - `__call__` now calls `build_middleware_stack()` itself and sets `scope["app"]`. Check that this doesn't change behavior for subclasses or for a custom `middleware_stack`.
   - `BackgroundTasks.__call__` bypasses Starlette's `BackgroundTask.__call__` loop. The tests cover custom `BackgroundTask` subclasses, but a subclass overriding `BackgroundTasks.__call__` upstream is a risk.
6. **Routing duplication.** `_route_selected` is repeated across five routing paths. The redirect branch re-walks `_IncludedRouter._match` to find the final route (`routing.py`, redirect hunk), which could mismatch the real match for unusual routers. Mount handling uses string prefix concatenation and `removesuffix("/{path}")`.
7. **Thread-safety and global state.** `_runtime.py` uses a module-level `_owned` and `_configured` list under an `RLock`. `_flush` iterates `_owned` without taking the lock. Multiple apps in one process share this state.
8. **Tests.** `pyproject.toml` adds `logfire`, `sentry-sdk` and `opentelemetry-instrumentation-fastapi` to the test group. It also sets `patch = ["subprocess", "_exit"]` for coverage. That's a lot of third-party test coupling.
9. **Process.** The PR body says it was written with AI help and manually reviewed. It also records a 2026-09-29 note that Sentry and Logfire were checked and aren't blocked.

## Uncertainty

I did not read the remaining `_asgi.py` logic: attribute names, how semantic conventions are applied, how `finish` handles errors, and the per-request overhead. I also did not run the tests or check CI. The points above that depend on `_asgi.py` (items 1 and 3) are inferred from the signatures, the grep output and the docs text, and need confirmation by reading that file.