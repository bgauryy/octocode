**Summary:** PR #16403 is merged. Its head SHA is `daba280d`, and it changes 36 files (+5251/−46). It makes OpenTelemetry on by default in every `FastAPI()` app. It adds spans, metrics and logs with no opt-in. I read the diff and the `_runtime.py`, `_api.py` and part of `_asgi.py` sources. I did not read the tests or most of `_asgi.py`, and I did not run anything.

## Runtime behavior changes

1. **New hard dependency.**
   - `opentelemetry-api>=1.44.0` is now a core dependency (`pyproject.toml` diff).
   - The SDK and OTLP/HTTP exporter go into a new `opentelemetry` extra. They are also added to the `standard`, `standard-no-fastapi-cloud-cli` and `all` extras.

2. **New constructor argument.**
   - `FastAPI(telemetry=TelemetryConfig)` takes these keys: `tracer_provider`, `meter_provider`, `logger_provider`, `tracing`, `metrics`, `logs`, `operation_spans`, `auto_configure` and `exclude`.
   - Every feature defaults to on (`fastapi/applications.py`, `self._telemetry` block).

3. **`FastAPI.__call__` is rewritten** (`fastapi/applications.py`).
   - Lifespan scopes go through `fastapi.telemetry._runtime.lifespan`.
   - HTTP and WebSocket scopes go through `NativeTelemetry` unless the scope is excluded, already carries `"fastapi.telemetry"`, or `enabled()` is false.
   - On that path it builds the middleware stack early and sets `scope["app"]`.
   - A new `ExceptionTelemetryMiddleware` sits right after `ServerErrorMiddleware`.

4. **Telemetry emitted** (`_asgi.py`, `routing.py`).
   - A server span named `METHOD /route`, renamed once the route is known.
   - The `http.server.request.duration` histogram and the `http.server.active_requests` up/down counter.
   - WebSocket spans.
   - Warning logs for validation failures, and error logs for unhandled exceptions. Per the docs, error logs are emitted even when the trace is not sampled.

5. **Operation spans** (when `operation_spans` is true; on by default).
   - The span names are `fastapi.dependencies`, `fastapi.endpoint`, `fastapi.serialization` and `fastapi.background_task`.
   - Sync endpoints now run through `_run_sync_endpoint` inside `run_in_threadpool` (`routing.py`).
   - `BackgroundTasks.__call__` is overridden to wrap each task (`fastapi/background.py`).

6. **Route tracking.**
   - `_route_selected(...)` calls are added in about six places in `routing.py`: included routers, mounts, redirects, low-priority matches and the frontend router.
   - These set `http.route` and rename the span. Mount prefixes are accumulated.

7. **Exporter auto-configuration at lifespan startup** (`_runtime.py:85-207`).
   - If `OTEL_EXPORTER_OTLP_ENDPOINT` or a per-signal endpoint is set, it adds OTLP exporters.
   - When the global provider is still a proxy, it creates and installs an SDK provider. Otherwise it attaches a processor or reader to the existing provider.
   - It raises `FastAPIError` for non-`otlp` exporters, protocols other than `http/protobuf`, or invalid URLs.
   - It also raises if the SDK extra is missing.
   - Startup failures are sent as `lifespan.startup.failed`.
   - It flushes at shutdown, and an `atexit` shutdown is registered.
   - It is skipped when `OTEL_SDK_DISABLED=true`.

8. **Public hook.**
   - `get_telemetry_data()` exposes request, body, values and errors to log and span processors. For this it stores the `request`, `body`, `values` and `errors` on `TelemetryData` as the request is handled.
   - The data is only readable synchronously during the request (`_api.py:135-144`).

## What a reviewer should watch

- **Default-on and new core dependency.**
  - Every user gets spans, metrics and logs, and `opentelemetry-api` is now a core dependency.
  - The overhead is not quantified in the PR as far as I read. I didn't check for benchmarks.
  - The `exclude` callable is the only per-request opt-out.

- **Double instrumentation and double export.**
  - `_legacy_otel` detects the contrib `OpenTelemetryMiddleware` by matching module and class name strings (`_asgi.py:72-84`). This is brittle.
  - The docs say "configure each destination once". With `auto_configure` on, FastAPI adds its own exporter alongside any existing one, and `_runtime.py:88-91` says it does not deduplicate other components' exporters. That can send data twice.
  - It also special-cases Logfire's `ProxyMeterProvider` by string match (`_runtime.py:73-82`).

- **Sensitive data.**
  - Exception logs include the message and stack trace, and the docs warn they may contain secrets.
  - `TelemetryData.errors` includes the original input values (`_api.py:107-110`).
  - `url.query` is redacted only for keys in `_SENSITIVE_QUERY_PARAMETERS` (`_asgi.py:~295`). I did not read that list, so check it.

- **Hot-path changes.**
  - `_operation` wraps the endpoint, dependencies and serialization, and the sync endpoint path now goes through an extra wrapper. Check the no-telemetry cost: it returns a shared `nullcontext` when there is no request telemetry or tracer.
  - `FastAPI.__call__` may build the middleware stack earlier than before. Check how that interacts with the `middleware_stack`-is-`None` and `add_middleware` ordering.
  - The `routing.py` redirect branch re-walks `_IncludedRouter` matches. Check its correctness and cost.

- **Behavior change on startup.**
  - Misconfigured `OTEL_*` variables can now fail app startup with `FastAPIError`.
  - The lifespan wrapper intercepts messages, so check servers that don't support lifespan and apps with custom lifespans. The code comments say initialization errors would otherwise look like an unsupported-lifespan fallback.

- **Global-state limits.**
  - Providers are global. The docs say independent telemetry for mounted sub-apps is not guaranteed.
  - `_owned` and `_configured` are module-level and hold state across apps and tests.

- **Process hooks.**
  - `atexit` is registered at import time.
  - Flushing runs through `anyio.to_thread.run_sync` on lifespan shutdown.
  - `pyproject.toml` adds `coverage patch = ["subprocess", "_exit"]`, which is test infrastructure, but it was presumably needed for the subprocess tests.

- **Process and size.**
  - The PR is about 5k lines and was AI-assisted ("Codex with gpt-6-astra, manually reviewed", per the PR body).
  - The PR body says Sentry and Logfire were consulted on 2026-09-29.
  - Test deps now include `logfire`, `sentry-sdk` and `opentelemetry-instrumentation-fastapi`. This makes the test environment sensitive to their versions.

## Uncertainty

I did not read the full `_asgi.py` (metrics attributes, status handling, `_SENSITIVE_QUERY_PARAMETERS`) or the tests. The performance and ordering concerns above come from reading the diff only. I did not measure them.