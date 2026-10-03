PR #16403 was merged on 2026-09-26 (merge commit `4b3949cd`). It makes OpenTelemetry on by default in every `FastAPI()` app. I read the diffs for `applications.py`, `routing.py`, `background.py`, `telemetry/_api.py`, `telemetry/_runtime.py` and `pyproject.toml`. I did not read `fastapi/telemetry/_asgi.py` (446 lines), which holds the span and metric logic, or any of the tests. Everything below about span and metric content is therefore inferred from the config docs and call sites.

## Runtime behavior changes

**New `telemetry=` argument.**
- `FastAPI(telemetry=TelemetryConfig)` defaults to `tracing`, `metrics`, `logs`, `operation_spans` and `auto_configure` all `True`, with providers taken from the global OTel state (`applications.py`, `self._telemetry` block).
- The only way to opt out is per app, for example `telemetry={"tracing": False}`.
- `opentelemetry-api>=1.44.0` is now a hard dependency in `pyproject.toml`. `opentelemetry-sdk` and the OTLP http exporter are added to the `opentelemetry`, `standard`, `standard-no-fastapi-cloud-cli` and `all` extras.

**`FastAPI.__call__` dispatch** (`applications.py`):
- For `lifespan` scopes, it wraps the app in `telemetry._runtime.lifespan`.
- For `http` and `websocket` scopes, it goes through `NativeTelemetry` unless one of these holds:
  - the scope already has a `fastapi.telemetry` key;
  - telemetry is disabled;
  - the scope type is something else.
- It calls `build_middleware_stack()` early if needed, and sets `scope["app"]` itself.
- It passes `legacy_otel=_legacy_otel(self.middleware_stack)` into `NativeTelemetry`. I infer this is meant to avoid double instrumentation when `opentelemetry-instrumentation-fastapi` is also in use, but I didn't read `_asgi.py` to confirm.

**Middleware stack.** `ExceptionTelemetryMiddleware` is inserted right after `ServerErrorMiddleware`, so it sits in every app's stack.

**Operation spans** (`routing.py`, `background.py`), named `fastapi.<name>`:
- `dependencies`, `endpoint` and `serialization` spans wrap dependency resolution, the endpoint call and response serialization.
- `endpoint` spans also cover the WebSocket handler.
- Sync endpoints now run via `_run_sync_endpoint` in the threadpool, so the span is created inside the worker thread.
- `BackgroundTasks` gets a new `__call__` override that opens a `background_task` span per task.
- Spans carry `code.function.name`. `error.type` and ERROR status are set on non-HTTP exceptions, and always for background tasks.

**Route naming.** `_route_selected` is called from many places in `routing.py`: `_handle_selected`, the included-router handler, `Mount`, the redirect-slashes path and the low-priority match. It sets `http.route` and renames the span to `METHOD route`. Mounts accumulate a `_mount_prefix`.

**Telemetry data and logs.**
- `TelemetryData` holds `request`, `websocket`, `body`, `values` and `errors`. It is exposed via `get_telemetry_data()` so processors from Logfire or Sentry can read it synchronously.
- Validation failures emit a `fastapi.validation.failed` WARN log record.
- `logs` also records unhandled exceptions with messages and stack traces, per the config docstring.

**Lifespan and env auto-configuration** (`_runtime.py`):
- On `lifespan.startup`, if `auto_configure` is on and `OTEL_SDK_DISABLED` isn't `true`, it adds OTLP exporters from `OTEL_*` environment variables.
- It only acts when an endpoint is configured. The endpoint comes from `OTEL_EXPORTER_OTLP[_SIGNAL]_ENDPOINT`.
- If no global provider is set (still a proxy provider), it creates an SDK provider and calls `set_*_provider`.
- Otherwise it attaches a processor or reader to the existing provider.
- Configuration errors send `lifespan.startup.failed` and raise, so the app fails to start.
- There is an `atexit` shutdown of the components FastAPI created, and a `force_flush` on lifespan shutdown and failure events, run in a thread.

## What a reviewer should watch

1. **Default-on behavior.** This is a hot-path change for everyone. With no SDK installed it should be near no-op, since `_operation` returns a shared `nullcontext` when there is no request telemetry. The per-request cost of `NativeTelemetry.enabled()` and `_legacy_otel(self.middleware_stack)` is something I didn't check.
2. **Startup can now fail from environment variables alone.**
   - A non-`otlp` exporter, a protocol other than `http/protobuf`, an invalid endpoint, or a missing SDK extra all raise `FastAPIError`.
   - Examples: `OTEL_TRACES_EXPORTER=console`, or `OTEL_EXPORTER_OTLP_PROTOCOL=grpc`.
   - Those settings can be legitimate for users running other OTel tooling, and they must now know to set `auto_configure: False`.
3. **Global side effects.** FastAPI calls `set_tracer_provider`, `set_meter_provider` and `set_logger_provider`, and registers an `atexit` hook. There is a race-handling path for concurrent configuration. `_configured` and `_owned` are module-level state, shared across apps and tests.
4. **Provider-detection brittleness.** `_unconfigured` compares the provider's module and class name against a hard-coded set of private OTel classes (`_ProxyMeterProvider`, `ProxyLoggerProvider`). `_registration_provider` special-cases `logfire._internal.metrics.ProxyMeterProvider`. These can break when OTel or Logfire are upgraded.
5. **Sensitive data.**
   - The `logs` option records exception messages and stack traces, and `TelemetryData` exposes the body, validated values and validation errors. The errors include original input values.
   - The docs say this isn't auto-exported, but it is reachable from processors. `logs=True` by default means exception details go to the configured backend by default.
6. **Behavior changes in the request path.**
   - Sync endpoints now run through a wrapper in the threadpool.
   - `telemetry_data.body` is set before dependency resolution. `values` and `errors` are set from `solved_result`.
   - `BackgroundTasks.__call__` no longer delegates to Starlette's implementation. Confirm that ordering and exception semantics are unchanged.
7. **Route-naming complexity.** `_route_selected` is scattered across about seven call sites, including a loop that unwraps `_IncludedRouter` for the redirect-slashes path. Mount and included-router `http.route` values are the likeliest place for wrong or high-cardinality route names.
8. **Test-suite weight.**
   - The PR adds about 5,250 lines, most of them tests.
   - The test dependency group now includes `logfire`, `sentry-sdk` and `opentelemetry-instrumentation-fastapi`.
   - Coverage is changed to `patch = ["subprocess", "_exit"]`.
9. **Follow-ups.** The description says the author plans to refactor internals soon. It also says the design was checked with Sentry and Logfire on 2026-09-29. A follow-up with OTel contrib is expected. The PR body discloses AI assistance with manual review.

## Uncertainty

I did not read `_asgi.py`, so these are unverified:
- the span and metric names and attributes;
- the `_legacy_otel` de-duplication logic;
- the `exclude` handling;
- the `ExceptionTelemetryMiddleware` details.

I also did not read the tests, so I can't say how well the points above are covered.