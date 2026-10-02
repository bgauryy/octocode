PR #16403 is merged. Its head is `daba280d4c82235edc81621ddb869b5bf3879198`. It changes 36 files (+5251/−46), and about 2,900 of those added lines are tests. It makes OpenTelemetry (OTel) instrumentation part of `FastAPI` itself, on by default, and it adds a hard dependency. I read the diffs for `applications.py`, `routing.py`, `background.py`, `pyproject.toml`, the docs page, `_runtime.py` and `_asgi.py` (first 260 lines). I did not read `_api.py`, the rest of `_asgi.py`, or the tests.

## Runtime behavior changes

1. **New `telemetry=` argument on `FastAPI()`.** It takes a `TelemetryConfig` dict with these keys: `tracer_provider`, `meter_provider`, `logger_provider`, `tracing`, `metrics`, `logs`, `operation_spans`, `auto_configure` and `exclude`. Everything is on by default (`fastapi/applications.py`, `__init__` diff).

2. **`FastAPI.__call__` now dispatches on scope type** (`applications.py`, `__call__` diff).
   - `lifespan` scopes go through `telemetry._runtime.lifespan`.
   - `http` and `websocket` scopes go through `NativeTelemetry` only if `enabled()` is true. That means at least one signal is on and a real, non-no-op provider is configured or passed in.
   - Anything else, or a scope that already has `"fastapi.telemetry"`, takes the old path.
   - On the telemetry path it builds the middleware stack eagerly and sets `scope["app"]`.

3. **`ExceptionTelemetryMiddleware` is added to every app's middleware stack.** It sits right after `ServerErrorMiddleware` and before user middleware (`applications.py`, `build_middleware_stack` diff). It catches `Exception`, records it, and re-raises (`_asgi.py:143-154`).

4. **Spans, metrics and logs.**
   - The docs list HTTP spans, the `http.server.request.duration` histogram and the `http.server.active_requests` counter (`_asgi.py:202-211`).
   - The docs also list WebSocket spans, error logs for unhandled exceptions, and warning logs for validation failures.
   - Unhandled-exception logs are emitted from `_exception` (`_asgi.py:87-113`).
   - The docs say error logs are recorded even when the trace is not sampled.

5. **Per-operation spans in `routing.py` and `background.py`.** `_operation(...)` wraps these steps:
   - dependency solving
   - the endpoint call
   - response serialization
   - each background task

   Sync endpoints now run through `run_in_threadpool(_run_sync_endpoint, function=..., arguments=...)`. `BackgroundTasks.__call__` is overridden to loop over tasks itself, instead of using Starlette's implementation. WebSocket handlers get the same wrapping.

6. **Route-template hooks.** `_route_selected(...)` is called at about 6 routing points. These include included routers, mounts, the redirect-slashes path, and the low-priority match, so spans and metrics can carry the route template. The routing handlers also stash `request`, `body`, `values` and `errors` on a `get_telemetry_data()` object. The PR description says this is for Sentry and Logfire, so they don't have to monkeypatch FastAPI.

7. **Startup behavior (`_runtime.py`).**
   - On `lifespan.startup`, with `auto_configure` on and `OTEL_SDK_DISABLED` not `true`, it reads the `OTEL_*` environment variables (`_runtime.py:85-110`).
   - If an OTLP endpoint is set, it creates SDK providers and exporters. It calls `set_tracer_provider`, `set_meter_provider` and `set_logger_provider` globally if the global provider is unconfigured. Otherwise it attaches an extra exporter to the existing provider (`_runtime.py:150-207`).
   - It registers an `atexit` shutdown (`_runtime.py:39`) and flushes on lifespan shutdown (`_runtime.py:226-237`).

8. **Legacy contrib detection.** `_legacy_otel()` walks the built middleware stack looking for `opentelemetry.instrumentation.asgi.OpenTelemetryMiddleware`. If it finds one, native tracing, metrics and logs are turned off to avoid double instrumentation (`_asgi.py:72-84`, `235-244`).

9. **Dependencies (`pyproject.toml`).**
   - `opentelemetry-api>=1.44.0` becomes a hard dependency.
   - SDK and OTLP-HTTP exporter are added to a new `opentelemetry` extra, and also to `standard`, `standard-no-fastapi-cloud-cli` and `all`.
   - Test dependencies add `logfire`, `sentry-sdk` and `opentelemetry-instrumentation-fastapi`.

## What a reviewer should watch for

- **Hot-path cost on every request.**
  - The routing code now has `with _operation(...)` and `get_telemetry_data()` calls even when telemetry is off.
  - `NativeTelemetry.enabled()` checks the providers on every call.
  - I did not read `_api.py`, so I can't say how cheap the no-op path is. This needs benchmarking.

- **Exception-handling semantics.**
  - The new middleware is inserted in front of user middleware, so it sees exceptions before the exception handlers run.
  - Check ordering and double logging for `HTTPException`, validation errors, `BaseException` and WebSocket disconnects.
  - The dedup is by exception identity (`_asgi.py:93`).

- **Sensitive data in logs.** The docs admit that exception messages and stack traces may contain sensitive data. Logs are enabled by default and the user has to redact them. The default query-string redaction list is short (`_SENSITIVE_QUERY_PARAMETERS`, `_asgi.py:44-52`). Check what else lands in span attributes, for example headers, URLs and route values.

- **Surprising global side effects.**
  - Setting `OTEL_EXPORTER_OTLP_ENDPOINT` now makes FastAPI install global providers and exporters at startup.
  - If another library also exports from the environment, data is duplicated. The docs tell users to turn off one of the two.
  - Non-OTLP exporters or non-`http/protobuf` protocols raise `FastAPIError` at startup (`_runtime.py:51-64`). Existing deployments that use `OTEL_TRACES_EXPORTER=console` or gRPC could fail to start once they upgrade. That is worth confirming.

- **Special-casing of vendors.** `_registration_provider` checks `logfire._internal.metrics.ProxyMeterProvider` by module and class name (`_runtime.py:73-82`). This is fragile against Logfire internals.

- **Behavior change in `BackgroundTasks`.** Overriding `__call__` bypasses Starlette's own loop, so check parity with it. Also check the docs claim that task spans stay in the request's trace after the response span has ended.

- **Mounted sub-apps.** The docs warn that independent telemetry per mounted app isn't guaranteed. The `"fastapi.telemetry"` scope key and `exclude` handling try to avoid double instrumentation. Mount, redirect and low-priority routing paths are the most complex hooks.

- **Dependency footprint.** `opentelemetry-api` is now required for everyone, and the SDK is in `standard`. Check the licensing and size implications, and the version floor of `>=1.44.0`.

- **Review confidence.** This is a large diff of AI-generated code (the PR body says Codex was used, then manually reviewed). The PR body says Sentry and Logfire were checked on 2026-09-29.

## Not verified

- `_api.py` and the second half of `_asgi.py` (span naming, attributes, metrics recording, WebSocket handling).
- Test results and CI status.
- The review discussion.
- Whether any follow-up fixes landed after merge.