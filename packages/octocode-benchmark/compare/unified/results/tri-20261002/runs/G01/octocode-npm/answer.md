**Answer:** PR #16403 (merged 2026-09-29, 36 files, +5251/−46) adds built-in OpenTelemetry tracing, metrics and logs to `FastAPI`. It's on by default and only does work when an OTel provider is configured. It also adds a hard dependency, `opentelemetry-api>=1.44.0`, and wraps the request path. I read the diffs for `applications.py`, `routing.py`, `background.py` and `pyproject.toml`. I read the new `_runtime.py` and `_asgi.py` from master (not the PR head), and I did not read `telemetry/_api.py` or the tests.

## Runtime behavior changes

**New `telemetry=` argument.** `FastAPI(telemetry=...)` takes a config with these defaults (`applications.py`, `__init__` hunk):
- `tracing`, `metrics`, `logs`, `operation_spans` and `auto_configure` are all `True`.
- The three providers default to `None` (use the global provider).
- `exclude` defaults to `None`.

**`FastAPI.__call__` now branches** (`applications.py`, `__call__` hunk):
- **Lifespan scopes** go through `telemetry._runtime.lifespan`.
- **http/websocket scopes** go through `NativeTelemetry` when `enabled()` is true and the scope has no `"fastapi.telemetry"` key.
- **Everything else** takes the original path.

**Spans, metrics and logs** (`_asgi.py`):
- **Server span:** it is named `HTTP`/`WS` plus the method, with `SpanKind.SERVER`, and the parent context is extracted from request headers.
- **Query redaction:** `url.query` redacts only five signature parameters, such as `X-Amz-Signature` and `sig`.
- **Metrics:** `http.server.request.duration` and `http.server.active_requests` are recorded. WebSockets get no metrics.
- **Exception logs:** unhandled exceptions are emitted as log records.
- **Failure rule:** a request counts as failed if the status is ≥500, the response is missing, or an exception was raised.

**Child spans.** The `_operation` wrapper creates spans named `endpoint`, `dependencies`, `serialization` and `background_task`.
- `routing.py` adds them around the endpoint call, `solve_dependencies` and `serialize_response`.
- `BackgroundTasks.__call__` in `background.py` is overridden to wrap each task.
- Sync endpoints now run through `_run_sync_endpoint` in the threadpool.

**Route and error hooks.**
- `_route_selected(...)` is called from many routing paths to record `http.route`.
- `_validation_failed(...)` is called before validation errors are raised.
- The request handler also stashes `request`, `body`, `values` and `errors` on the telemetry data object, which is how Sentry and Logfire get data without monkeypatching.
- `ExceptionTelemetryMiddleware` is inserted right after `ServerErrorMiddleware`, so it sees exceptions before error handlers turn them into responses.

**Contrib OTel interop.** `_legacy_otel` walks the built middleware stack. If it finds `opentelemetry.instrumentation.asgi.OpenTelemetryMiddleware`, native tracing, metrics and logs are all disabled for that request, which avoids double instrumentation.

**Auto-export at startup** (`_runtime.py`):
- With `auto_configure`, OTLP exporters are added from `OTEL_*` environment variables on `lifespan.startup`.
- Only `http/protobuf` and the `otlp` or `none` exporters are supported. Anything else raises `FastAPIError`, which is caught and logged as a warning, so startup still succeeds.
- Providers that FastAPI creates or adds components to are flushed on lifespan shutdown, and shut down via `atexit`.
- If the global provider is unconfigured, FastAPI creates and installs its own with `trace.set_tracer_provider`, `metrics.set_meter_provider` or `_logs.set_logger_provider`.

**Packaging** (`pyproject.toml`):
- `opentelemetry-api` becomes a core dependency.
- `opentelemetry-sdk` and `opentelemetry-exporter-otlp-proto-http` come in via a new `opentelemetry` extra, and are also added to `standard`, `standard-no-fastapi-cloud-cli` and `all`.

## What a reviewer should watch

1. **A new hard dependency on all installs, and the `standard` extra now installs the OTLP exporter.** If `OTEL_EXPORTER_OTLP_ENDPOINT` is set, startup can now install global providers and export data, even if the app never asked for telemetry.
2. **Process-global side effects.** Setting the global tracer, meter and logger providers is first-come-first-served. There is a concurrency fallback that shuts down the unused provider, and `_runtime.py` has special handling for Logfire's `ProxyMeterProvider`. This is fragile against vendor wrappers.
3. **Hot-path cost.** Every http/websocket request now calls `enabled()` and checks several provider types. Each request also gets extra `_operation` context managers, and sync endpoints get a different threadpool call signature. Check that the no-provider path is cheap, and that `_run_sync_endpoint` preserves contextvars and kwargs behavior.
4. **`BackgroundTasks.__call__` is overridden.** It replaces Starlette's loop. Confirm it matches Starlette's semantics for sync and async tasks and for `functools.partial`, since it calls `task()` directly.
5. **Exception handling in `__call__`.** `except BaseException` followed by `finish(...)` and re-raise covers cancellation. A cancellation or client disconnect is reported as a `ConnectionError` or an "Incomplete ASGI response" failure, which could create error noise.
6. **Privacy.** `url.path` and `url.query` are recorded on spans, and only five parameter names are redacted. Request `body` and `values` are held in per-request data (cleared in the `finally` block). Check that nothing exports them.
7. **Cardinality.** `_route_selected` appears in around eight routing branches, including mounts, redirects, low-priority matches and `_IncludedRouter`. This is the most error-prone area, because wrong routes cause high-cardinality `http.route` values. Unknown HTTP methods are bucketed as `_OTHER`, which is good.
8. **Interop and exclusion.** `exclude(scope)` sets `scope["fastapi.telemetry"] = None` so mounted FastAPI apps skip the request too. A mounted app inside an instrumented parent depends on the `"fastapi.telemetry" in scope` check.
9. **Process-level patching.** `[tool.coverage.run] patch = ["subprocess", "_exit"]` is added, and the tests group gains `logfire`, `sentry-sdk` and `opentelemetry-instrumentation-fastapi` as test dependencies.
10. **Process notes.** The PR was AI-assisted with a manual review, and the page shows no formal reviews (reviews totalItems 0). It is large (5k lines, about 4.5k of them tests and docs), so review depth was probably limited. The author notes that Sentry and Logfire were consulted and that this doesn't block them.

**Uncertainty:**
- `_runtime.py` shows a last-modified date of 2026-09-30, after the merge, so it may differ slightly from the merged PR.
- I did not read `telemetry/_api.py`, the helpers (`_operation`, `_run_sync_endpoint`, `_route_selected`), or the tests.
- Statements about their behavior come from call sites only.
- Per-line numbers are not given, since I viewed diffs and whole files without line anchors.