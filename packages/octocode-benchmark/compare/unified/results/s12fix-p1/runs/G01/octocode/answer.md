**Summary:** PR #16403 was merged on 2026-09-29 as `4b3949c`. It adds built-in OpenTelemetry tracing, metrics and logs to `FastAPI`. It's on by default, and it activates only if an OTel provider is configured or `OTEL_EXPORTER_OTLP_*` env vars are set. Everything below comes from the diff and the merged files. I didn't read the tests or the docs, and I didn't run anything.

## Runtime behavior changes

**New dependency and constructor option**
- `opentelemetry-api>=1.44.0` becomes a **hard core dependency** (`pyproject.toml:50`).
- The SDK and OTLP/HTTP exporter go into a new `opentelemetry` extra and into `standard`, `standard-no-fastapi-cloud-cli` and `all` (`pyproject.toml:61-64`, `66-67`, `87-88`, `107-108`).
- `FastAPI(telemetry=...)` is a new option (`applications.py`). It takes a `TelemetryConfig` with these keys and defaults:
  - `tracer_provider`, `meter_provider`, `logger_provider`: `None`, which means use the global provider.
  - `tracing`, `metrics`, `logs`: `True`.
  - `operation_spans`: `True`.
  - `auto_configure`: `True`.
  - `exclude`: `None`.

**Request path (`applications.py`, `FastAPI.__call__`)**
- Lifespan scopes now go through `telemetry._runtime.lifespan`.
- For `http` and `websocket` scopes, the request goes through `NativeTelemetry` only if telemetry is enabled. That means some signal is on and either an explicit provider is set or the global provider isn't the no-op default (`_asgi.py:172-196`). Otherwise the old path runs unchanged.
- Each telemetry-handled request:
  - Starts a SERVER span named `HTTP`/`WS`/method (`_asgi.py:262`, `311`) and extracts parent context from incoming headers (`283`).
  - Records `http.server.request.duration` and `http.server.active_requests` metrics. Metrics are skipped for websockets (`244`).
  - Emits log records for unhandled exceptions (`_asgi.py:87-113`).
  - Sets `http.route` from the matched route, and `error.type` plus ERROR status on 5xx, exceptions, or an incomplete response.
- The span name is only the method, not "METHOD /route". The route goes in `http.route` at finish.
- A new `ExceptionTelemetryMiddleware` is inserted into the middleware stack right after `ServerErrorMiddleware` (`applications.py`), so exceptions are seen before handlers convert them to responses.

**Child spans (`routing.py`, `background.py`)**
- With `operation_spans`, `_operation(...)` spans wrap dependency solving, the endpoint call, response serialization, websocket dependencies and endpoint, and each `BackgroundTasks` task.
- `BackgroundTasks.__call__` is now overridden to loop over tasks (`background.py`).
- Sync endpoints now run via `run_in_threadpool(_run_sync_endpoint, function=..., arguments=...)` instead of the bare call.
- `_route_selected(...)` is called at every routing decision. That covers mounts, the include-router path, redirect-slashes, and low-priority matches.
- The request handlers stash the request, body, solved values and errors on a per-request telemetry object (`telemetry_data`) for third-party consumers like Logfire and Sentry. The PR description says this is the purpose.

**Startup and shutdown (`_runtime.py`)**
- If an OTLP endpoint env var is set, the lifespan startup hook creates and registers OTLP exporters (`_runtime.py:85`).
  - It supports only `OTEL_*_EXPORTER=otlp` or `none`, and only the `http/protobuf` protocol.
  - If the global provider is unconfigured, it installs a new SDK provider as the global one. Otherwise it adds a processor or reader to the existing provider.
  - Misconfiguration raises `FastAPIError`, which is sent as `lifespan.startup.failed` (`_runtime.py:218-223`).
- It force-flushes on lifespan shutdown and shuts down the providers it created at `atexit` (`_runtime.py:39`, `236`).

**Interaction with existing OTel instrumentation**
- `_legacy_otel` walks the built middleware stack looking for contrib's `OpenTelemetryMiddleware`. If it finds one, native tracing, metrics and logs are all disabled for that request (`_asgi.py:72-84`, `235-244`).
- Mounted FastAPI apps that find `scope["fastapi.telemetry"]` already set skip native handling (`applications.py`).
- `exclude(scope)` suppresses telemetry for a request, including inside mounted apps (`_asgi.py:227-234`).

## What a reviewer should watch for

1. **Hot-path cost on every request.** The `__call__` change adds `enabled()` checks and provider lookups, plus `_operation` wrappers on every request even when telemetry is off. CodSpeed reported no change on the 24 existing benchmarks, but those may not cover the telemetry-enabled path.
2. **Hard dependency.** `opentelemetry-api` is now required for all users. Check version-pin conflicts with users' existing OTel stacks.
3. **Global side effects.** Auto-configure sets global tracer, meter and logger providers, registers an `atexit` hook, and changes lifespan behavior. Export is gated on env vars, and exporter errors raise during startup and prevent the app from starting. It's opt-out via `auto_configure: False`, not opt-in. This could surprise apps that already run `opentelemetry-instrument`. The docstring says FastAPI does not deduplicate against other components' exporters, so exporting twice is possible.
4. **Lifespan wrapping.** The `lifespan` wrapper adds a synchronous flush (in a thread) before `lifespan.shutdown.complete`, which can delay shutdown if the collector is slow. Servers that probe lifespan support depend on the wrapper preserving the failure and fallback semantics.
5. **Sync endpoints.** `_run_sync_endpoint` changes how they run in the threadpool. Confirm that contextvars and OTel context propagate correctly and that the signature and `functools` behavior are unchanged. I didn't read `_api.py`.
6. **Routing edits are broad.** There are about six `_route_selected` call sites, including a redirect branch that re-matches `_IncludedRouter` in a `while` loop. Check for a wrong `http.route` on mounts, redirects, or the 404/405 paths. High-cardinality routes would be a metrics problem.
7. **Privacy and cardinality.**
   - `url.path` and `url.query` are recorded on spans. Only a small allowlist of query keys is redacted (`X-Amz-*`, `sig`, `X-Goog-Signature`), so other secrets in query strings go out as-is (`_asgi.py:44-52`, `293-308`).
   - Metrics use `http.route`, which depends on routing correctness.
   - Request bodies, values and errors are held on the telemetry object. Check they aren't exported and are cleared (`_asgi.py:441-442`).
8. **Legacy and contrib detection is by class name.** `_legacy_otel` and `_registration_provider` match on module and class names for contrib and Logfire. That is fragile across versions. If detection fails, you may get duplicate spans or metrics. If instrumentation is applied after the stack is built, behavior depends on stack-build timing.
9. **Size and authorship.** The PR is +5,251/−46 across 36 files and was written with AI assistance. The author says they reviewed it manually. The only visible review activity is the bot comments, with no inline reviews. Sentry and Logfire compatibility was confirmed by the author, per the PR body. The test dev group now pulls in `logfire`, `sentry-sdk` and contrib instrumentation (`pyproject.toml:172-174`).

## Uncertainty
- I did not read `_api.py` (`_operation`, `_run_sync_endpoint`, `TelemetryConfig`, the websocket-disconnect logic) or the tests.
- The `applications.py` and `routing.py` patches were read in minified form, with hunks elided.
- I can't say whether any behavior is wrong, only where to look.