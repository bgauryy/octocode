**Summary:** PR #16403 (merged, head `daba280`) adds built-in OpenTelemetry to FastAPI. It is on by default, and it is active as soon as an OTel provider is configured globally or passed in. It adds server spans, metrics and logs for HTTP and WebSocket requests. It can also register OTLP exporters at lifespan startup, based on environment variables. I read the diff (`gh pr diff`) and did not run the code or the tests. Line numbers below refer to the diff, not to files at a commit.

## Runtime changes

1. **New `FastAPI(telemetry={...})` option** (`fastapi/applications.py`). The keys are `tracer_provider`, `meter_provider`, `logger_provider`, `tracing`, `metrics`, `logs`, `operation_spans`, `auto_configure` and `exclude`. All signals default to on. `fastapi/telemetry/__init__.py` exports `TelemetryConfig`, `TelemetryData` and `get_telemetry_data`.

2. **`FastAPI.__call__` is rewritten** (`applications.py`).
   - Lifespan scopes go through `telemetry._runtime.lifespan`.
   - HTTP and WebSocket scopes go through `NativeTelemetry`. This is skipped if a `fastapi.telemetry` key is already in the scope (a mounted app) or if `enabled()` is false.
   - `enabled()` is false when no signal has either a passed-in provider or a configured global provider (`_asgi.py`, `enabled`).

3. **New middleware in the stack.** `ExceptionTelemetryMiddleware` is inserted right after `ServerErrorMiddleware` in `build_middleware_stack`. It records unhandled exceptions as OTel logs, even when the trace is not sampled, according to the docs.

4. **Spans, metrics and logs.**
   - **Server span:** named like `GET /items/{id}` or `WS /ws/{room}`. It takes its parent from propagated headers.
   - **Metrics:** request duration and an active-requests counter. These are HTTP only.
   - **Validation failures:** logged as warnings with the route and error count, not the input.
   - **Operation child spans** (`dependencies`, `endpoint`, `serialization`, `background_task`), switchable with `operation_spans`.

5. **Changes to hot paths in `routing.py`:**
   - `run_endpoint_function` wraps coroutine endpoints in `_operation(...)`.
   - Sync endpoints now run through `run_in_threadpool(_run_sync_endpoint, function=..., arguments=...)` instead of calling the function directly.
   - `solve_dependencies` and `serialize_response` are wrapped.
   - `_route_selected(...)` is called in about six route-matching branches. They are the included-router handler, frontend routes, router `app` full and partial matches, the trailing-slash redirect, and low-priority routes. This sets the route template on the span.
   - The request handler stores `request`, `body`, `values` and `errors` on the `TelemetryData` object.

6. **`BackgroundTasks.__call__` is overridden** (`background.py`). Each task runs inside a span, and the loop no longer calls Starlette's implementation.

7. **Startup behavior.** When `auto_configure` is true and `OTEL_SDK_DISABLED` is not `true`, `_configure_from_environment` runs on `lifespan.startup` (`_runtime.py`).
   - It reads `OTEL_EXPORTER_OTLP_*` endpoints and adds OTLP http/protobuf exporters to the providers.
   - If the global provider is an unconfigured default, it creates and installs an SDK provider with `set_*_provider`.
   - It flushes owned components on shutdown events and registers an `atexit` shutdown.

8. **Dependencies** (`pyproject.toml`).
   - `opentelemetry-api>=1.44.0` becomes a hard dependency of the core package.
   - `opentelemetry-sdk` and `opentelemetry-exporter-otlp-proto-http` are added to the `standard`, `standard-no-fastapi-cloud-cli` and `all` extras, and to a new `opentelemetry` extra.

## What a reviewer should watch

- **Default-on, implicit global side effects.** Setting `OTEL_EXPORTER_OTLP_ENDPOINT` makes FastAPI install global providers. It also adds a second exporter if another library already exports to the same place. The docs only say to "configure each destination once" and to use `auto_configure: False`. Check for duplicated telemetry with Logfire, Sentry or contrib.

- **Startup can now fail on configuration.** `_export_endpoint` raises `FastAPIError` for any exporter other than `otlp` or `none`, for any protocol other than `http/protobuf`, and for invalid URLs. `lifespan` turns that into `lifespan.startup.failed`. Apps with gRPC OTLP or other `OTEL_*` settings could break on upgrade.

- **Duplicate instrumentation guard is heuristic.** `_legacy_otel` walks the built middleware stack for contrib's `OpenTelemetryMiddleware`, matching on module and class name, and disables native tracing, metrics and logs if it finds one. Note the comment in the diff: contrib instrumentation added after the stack is built isn't detected. The same `_unconfigured` check matches provider classes by name in `_DEFERRED_PROVIDERS`. `_registration_provider` has a special case for Logfire's `ProxyMeterProvider`. Both can break when those libraries change.

- **Data exposure.**
  - `url.query` is recorded on spans, with only keys in `_SENSITIVE_QUERY_PARAMETERS` redacted. I didn't see the contents of that set.
  - Exception logs include the message and stack trace. The docs warn this can be sensitive and point to log processors.
  - `get_telemetry_data()` exposes `request`, `body`, `values` and `errors` to third parties.

- **Hot-path and semantics changes.**
  - Sync endpoints now go through a wrapper in the threadpool. Check context propagation and the span parent.
  - `BackgroundTasks.__call__` no longer delegates to Starlette, so it could diverge from upstream behavior. For example, Starlette's `BackgroundTask` handles sync and async functions itself, and the new loop only awaits `task()`.
  - The `_route_selected` calls are scattered across the branches listed in item 5. A missed branch means a wrong or missing route name on the span.
  - CodSpeed reported "24 untouched benchmarks, no alteration", but the comparison base was `79d42b2`, not `master` (`c30032a`), because no successful master run was found.

- **Mounted sub-apps and `exclude`.** The docs say independent telemetry configuration for sub-apps "is not guaranteed". `exclude` works by setting `scope["fastapi.telemetry"] = None` and popping it afterwards.

- **Process-global state** (`_runtime.py`): a module-level lock, `_owned`, `_configured`, and `atexit`. Check behavior with multiple app instances, reloads and tests. `_flush` runs through `anyio.to_thread.run_sync`, which blocks shutdown events until the flush finishes.

- **Review coverage.** The `reviews` field on the PR was empty. The author's note says it was made with AI assistance and manually reviewed, and that Sentry and Logfire confirmed it doesn't block them (2026-09-29).

## Not verified
I didn't read the whole of `_api.py`, `_asgi.py` (only the parts quoted above) or the tests. I didn't check the contents of `_SENSITIVE_QUERY_PARAMETERS`, `_DEFERRED_PROVIDERS` or the span-ending and exception-recording code. I didn't run anything.