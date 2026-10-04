**Short answer:** PR #16403 is merged. It makes OpenTelemetry on by default in FastAPI. Every HTTP and WebSocket request is now wrapped in telemetry code, and `opentelemetry-api` becomes a hard dependency. The diff is 36 files, +5251/−46, and head SHA is `daba280d`. I read the diffs of `applications.py`, `routing.py`, `background.py` and `pyproject.toml` in full. I only grepped `_runtime.py` and `_asgi.py`, and I did not read the tests.

## Runtime behavior changes

1. **New `telemetry=` argument on `FastAPI(...)`.**
   - It takes a `TelemetryConfig` dict (`applications.py` hunk @@ -58). Defaults are `tracing`, `metrics`, `logs`, `operation_spans` and `auto_configure` all `True`. `exclude` and the three providers default to `None`.
   - The defaults are merged with the user's dict and a `NativeTelemetry` instance is created (`applications.py`, hunk @@ -1011).
   - Telemetry is on unless you opt out.

2. **`FastAPI.__call__` is rewritten** (`applications.py`, hunk @@ -1160).
   - `lifespan` scopes go through `fastapi.telemetry._runtime.lifespan`.
   - `http` and `websocket` scopes go through `NativeTelemetry` unless `"fastapi.telemetry"` is already in the scope or telemetry is disabled.
   - The middleware stack is built eagerly there if needed, and `scope["app"]` is set manually.

3. **New middleware.** `ExceptionTelemetryMiddleware` is inserted right after `ServerErrorMiddleware` in `build_middleware_stack`. This adds a layer to every app's middleware stack, even when telemetry is disabled.

4. **Spans, metrics and logs.**
   - There are HTTP and WebSocket spans, plus HTTP metrics (duration and active requests).
   - Unhandled exceptions and validation failures become OTel logs.
   - Per the docs, error logs are recorded even for unsampled traces. Exception logs include the message and stack trace, which can contain sensitive data.

5. **Per-operation spans** (`operation_spans`, default on) are added in `routing.py` and `background.py`:
   - `dependencies` wraps `solve_dependencies`, for both HTTP and WebSocket.
   - `endpoint` wraps the async endpoint call.
   - `serialization` wraps `serialize_response`.
   - `background_task` is new, from an overridden `BackgroundTasks.__call__` (`background.py:+63`).

6. **Sync endpoints** now run through `_run_sync_endpoint` inside `run_in_threadpool` (`routing.py`, hunk @@ -349).

7. **Route-template capture.** `_route_selected(...)` calls are added at 5 or more routing sites. They are in `_handle_selected`, a `handle` method, `app()` for full, partial and redirect matches, and low-priority matches. They supply the `GET /items/{item_id}` span name. The redirect and nested `_IncludedRouter` path adds a manual match loop that duplicates routing logic.

8. **Request data exposed to telemetry.** The handler stores the `request`, the parsed `body`, the solved `values` and the `errors` on a telemetry-data object (`routing.py`, hunk @@ -404 and the `telemetry_data.*` assignments). The PR body says this is so Sentry and Logfire don't have to monkeypatch FastAPI.

9. **Environment auto-configuration** (`_runtime.py`).
   - On lifespan startup it reads `OTEL_*` variables and `OTEL_SDK_DISABLED`. It adds OTLP exporters to the selected providers, and creates and **globally registers** SDK providers if none exist (`_runtime.py:~160-171`).
   - It uses `shutdown_on_exit=False` and registers its own `atexit` shutdown (`_runtime.py:39`).
   - It supports only `OTEL_*_EXPORTER=otlp` or `none`, and warns otherwise (`_runtime.py:53`).

10. **Legacy contrib instrumentation.** `_legacy_otel(self.middleware_stack)` detects `opentelemetry-instrumentation-fastapi` and avoids double instrumentation (`_asgi.py:72-79`).

11. **Dependencies** (`pyproject.toml`).
    - `opentelemetry-api>=1.44.0` becomes a core dependency.
    - `opentelemetry-sdk` and the OTLP HTTP exporter are added to the `standard`, `standard-no-fastapi-cloud-cli` and `all` extras, and to a new `opentelemetry` extra.
    - The `tests` group adds `logfire`, `sentry-sdk` and the contrib FastAPI instrumentation.
    - Coverage config adds `patch = ["subprocess", "_exit"]`.

## What a reviewer should watch

- **Default-on with a new core dependency.** Every FastAPI install now pulls in `opentelemetry-api`. A `standard` install also gets the SDK and exporter, and it will export if `OTEL_EXPORTER_OTLP_ENDPOINT` is set. Check that behavior is unchanged when no endpoint is configured.
- **Global state.** The PR registers global tracer, meter and logger providers and an `atexit` hook. This can conflict with Logfire, Sentry, the contrib instrumentation or user setups, and can cause double export. The docs admit "Configure each destination once". Mounted sub-apps are explicitly "not guaranteed" to be independent.
- **Hot-path overhead and ordering.** The new code runs on every request. The rewritten `__call__` builds the middleware stack eagerly and sets `scope["app"]`. Check this against Starlette's own `__call__`, and check lifespan handling, which now goes through a wrapper.
- **Behavior changes unrelated to telemetry.**
  - `BackgroundTasks.__call__` is overridden, so any subclass or custom behavior needs a check.
  - Sync endpoints now go through a wrapper function.
  - The extra middleware appears in `user_middleware`/stack introspection tests.
- **Privacy.**
  - Exception messages and stack traces are exported as logs, even for unsampled traces. The docs tell users to filter or redact them.
  - `request`, `body` and `values` are held in a telemetry-data object.
- **Routing duplication.** The redirect and nested-router branch re-implements matching (`routing.py`, hunk @@ -2753). It could drift from the real routing logic.
- **Size and generation.** The PR is about 5k lines, mostly new `telemetry/*` modules and tests. The author says it was AI-assisted and manually reviewed. The author also says the internals will be refactored soon, so the data-exposure hooks are probably not a stable API.
- **Env-var edge cases.**
  - The code reads `OTEL_SDK_DISABLED`, the per-signal endpoint and protocol variables, and `OTEL_INSTRUMENTATION_HTTP_KNOWN_METHODS`.
  - Check precedence, and what happens for `grpc` or `http/json` protocols.

## Uncertainty

- I did not read `_api.py`, the body of `_asgi.py`, or most of `_runtime.py`. I inferred their behavior from grep hits and the docs.
- I did not run the tests or measure the overhead.
- Line numbers cited for `routing.py` and `applications.py` are diff hunk headers, not lines at the pinned commit. The `_runtime.py` and `_asgi.py` line numbers are grep line numbers in the file at `daba280d`.