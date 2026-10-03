**Summary:** PR #16403 (merged, head `daba280d`) makes OpenTelemetry on by default in every `FastAPI()` app. It adds a required `opentelemetry-api` dependency. It wraps every HTTP and WebSocket request and the ASGI lifespan. I read the diff (`gh pr diff`) and skimmed the code. I did not run the tests or the code. The line numbers below are diff-relative (I saved the diff as `/tmp/p.diff`), not file lines at the pinned commit.

## Runtime behavior changes

**1. New `telemetry=` argument (`fastapi/applications.py`).**
- The config defaults are `tracing`, `metrics`, `logs`, `operation_spans` and `auto_configure`, all `True`, plus `exclude=None` and the three `*_provider` options as `None`.
- Global providers are used unless you pass your own.
- `FastAPI.__call__` is rewritten. Lifespan scopes go through `telemetry._runtime.lifespan`.
- HTTP and WebSocket scopes go through `NativeTelemetry`. This is skipped when the scope already has `"fastapi.telemetry"` or telemetry is disabled.
- `ExceptionTelemetryMiddleware` is inserted right after `ServerErrorMiddleware`, so exceptions are seen before handlers turn them into responses.

**2. Signals recorded (`_asgi.py`, docs).**
- **Traces:** a SERVER span per HTTP request named like `GET /items/{item_id}`, and one per WebSocket named like `WS /ws/{room}`.
- **Metrics:** `http.server.request.duration` and `http.server.active_requests`, HTTP only.
- **Logs:** unhandled exceptions, recorded even when the trace is unsampled, and validation failures as warnings.
- **Operation spans:** `dependencies`, `endpoint`, `serialization` and `background_task`. These are added in `routing.py` through `_operation(...)` context managers.

**3. Hot-path edits in `routing.py`.**
- Sync endpoints now run through `run_in_threadpool(_run_sync_endpoint, ...)` instead of calling the function directly.
- `solve_dependencies` and `serialize_response` are wrapped.
- `_route_selected(...)` is called at every routing branch to name the span with the route template. The branches are included routers, the frontend route, the `Mount` and partial-match paths, the redirect-slash path and low-priority routes.
- The redirect-slash branch has a new loop that re-walks `_IncludedRouter` matches.
- `telemetry_data.request`, `.body`, `.values` and `.errors` are stashed per request.
- `fastapi.background.BackgroundTasks.__call__` is overridden to run each task inside a span.

**4. Lifespan and auto-configuration (`_runtime.py`).**
- **Environment export:** at `lifespan.startup`, if `OTEL_EXPORTER_OTLP_*ENDPOINT` is set, it adds OTLP exporters to the global providers.
- **Unsupported config:** it raises `FastAPIError` and sends `lifespan.startup.failed` for any exporter other than `otlp` or `none`, or any protocol other than `http/protobuf`.
- **Flushing:** it registers an `atexit` shutdown. It also flushes the components it created on lifespan shutdown, using a thread.

**5. Dependencies (`pyproject.toml`).**
- `opentelemetry-api>=1.44.0` becomes a hard dependency.
- The SDK and OTLP/HTTP exporter are added to the `opentelemetry`, `standard`, `standard-no-fastapi-cloud-cli` and `all` extras.
- The test group now also pulls in `logfire`, `sentry-sdk` and `opentelemetry-instrumentation-fastapi`.

## What a reviewer should watch

- **Default-on with core dependency.** Every app pays the per-request wrapping cost, even if no SDK is installed. The no-SDK path presumably uses no-op providers (`_unconfigured` in `_api.py`). I did not verify that it is cheap.
- **Startup can now fail because of environment variables.** An unsupported `OTEL_*_EXPORTER` or `OTEL_EXPORTER_OTLP_PROTOCOL` value, a bad endpoint URL, or a missing SDK with an endpoint set raises at lifespan startup. Existing deployments that already have OTEL env vars (for example `OTEL_TRACES_EXPORTER=console` or grpc) could break on upgrade. The workaround is `auto_configure=False`.
- **Duplicate exports.** The code does not inspect other exporters. If Logfire, Sentry or contrib already export to the same environment endpoint, data is sent twice. The docs say to "configure each destination once".
- **Legacy contrib instrumentation.** `_legacy_otel` detects `opentelemetry.instrumentation.asgi.OpenTelemetryMiddleware` by walking the built stack, using module and class name strings. When it finds it, native tracing, logs and metrics are turned off. That check is brittle. It also runs on every request, and I did not check whether the result is cached.
- **Sensitive data.**
  - Exception logs include the message and stack trace, which can be sensitive.
  - `url.query` is recorded with only a fixed list of parameter names redacted (`_SENSITIVE_QUERY_PARAMETERS`). I did not read that list, so check that it covers what you need.
  - Request bodies and dependency values are kept on the per-request telemetry data object, which third-party integrations can read.
- **Behavior around existing code.**
  - The sync endpoint path goes through a different function.
  - `BackgroundTasks.__call__` is overridden, so it no longer delegates to Starlette's implementation. Check that it behaves the same on errors and with sync tasks.
  - Mounted sub-apps share global providers, and the docs say independent configuration is not guaranteed.
- **Shutdown and flush.** Flushing runs `force_flush` in a thread through anyio, and only for components FastAPI created, not the user's providers. Check this under lifespan failure and under servers that do not send lifespan events.
- **Review effort.** The diff is about 5,250 additions, mostly tests. The PR body says it was written with AI assistance and manually reviewed. Sentry and Logfire confirmed on 2026-09-29 that it does not block them.

## Uncertainty
- I did not read `_api.py` (`_operation`, `_unconfigured`, `_route_selected`) or most of `_asgi.py` line by line. The cost of the no-SDK path, the exact list of redacted query parameters, and how `exclude` works for mounts are unverified.
- I did not check out the repo at the pinned commit, so there are no file line numbers.