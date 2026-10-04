**Bottom line:** PR #16403 was merged on 2026-09-29 as `4b3949cd` and adds built-in OpenTelemetry tracing, metrics and logs to FastAPI. It is on by default, but it only does work when an OTel provider is configured. I read the diffs for `applications.py`, `routing.py`, `background.py` and `pyproject.toml`, plus `_runtime.py` in full and lines 1–200 of `_asgi.py`. I did not read `_api.py`, the rest of `_asgi.py` (lines 200–446), the tests or the docs.

## Runtime behavior changes

1. **New `telemetry=` argument on `FastAPI()`** (`applications.py`).
   - The defaults are tracing, metrics, logs and operation spans all `True`, `auto_configure=True`, and global providers.
   - The merged config is stored as `self._telemetry`, and a `NativeTelemetry` object is created from it.

2. **`FastAPI.__call__` is rerouted** (`applications.py`).
   - Lifespan scopes are wrapped by `telemetry._runtime.lifespan`.
   - HTTP and websocket scopes go through `NativeTelemetry`, unless one of three things is true:
     - the scope already has `"fastapi.telemetry"`;
     - `enabled()` is false;
     - the scope type is something else.
   - In those cases the original `super().__call__` path runs unchanged.
   - `enabled()` is true only if tracing, metrics or logs is on and either an explicit provider is passed or the global provider is not the no-op one (`_asgi.py:172-196`).
   - So with no provider configured, the request path is essentially unchanged.

3. **A new middleware always sits in the stack.**
   - `ExceptionTelemetryMiddleware` is inserted right after `ServerErrorMiddleware`.
   - It records exceptions before the error handlers turn them into responses (`_asgi.py:143-154`).
   - It is added even when telemetry is disabled.
   - It emits an OTel log record per unhandled exception, and only if the request has telemetry state attached.

4. **Operation spans wrap the pipeline in `routing.py` and `background.py`.**
   - Spans named `endpoint`, `dependencies`, `serialization` and `background_task` are added.
   - Sync endpoints now run through `_run_sync_endpoint` instead of calling `run_in_threadpool(dependant.call, ...)` directly.
   - The websocket handler gets the same `dependencies` and `endpoint` spans.
   - The request handler stores `request`, `body`, `values` and `errors` on a per-request telemetry object, and the websocket handler stores `websocket`, `values` and `errors` on it.
   - `_validation_failed(...)` is called before validation errors are raised.
   - `_route_selected(...)` is called at about seven routing sites, to record `http.route`. The sites cover included routers, mounts, redirects and the low-priority match.
   - `BackgroundTasks.__call__` is overridden, so it no longer uses Starlette's loop.

5. **Lifecycle and env-based auto-export** (`_runtime.py`).
   - On `lifespan.startup`, `_configure_from_environment` may create SDK providers and OTLP exporters from the `OTEL_*` environment variables, and may call `trace.set_tracer_provider`, `metrics.set_meter_provider` or `_logs.set_logger_provider` globally.
   - It supports only the `otlp` exporter and only the `http/protobuf` protocol. Anything else raises `FastAPIError` (`_runtime.py:42-70`).
   - `OTEL_SDK_DISABLED=true` skips auto-configuration.
   - A failure sends `lifespan.startup.failed`, so a bad OTEL environment variable can stop the app from starting (`_runtime.py:216-224`).
   - On shutdown, `force_flush` runs in a thread (`_runtime.py:226-237`). There is also an `atexit` shutdown hook.

6. **Dependencies** (`pyproject.toml`).
   - `opentelemetry-api>=1.44.0` becomes a hard core dependency.
   - `opentelemetry-sdk` and `opentelemetry-exporter-otlp-proto-http` come in through a new `opentelemetry` extra and through `standard`.

## What a reviewer should watch

- **The default is on, with `opentelemetry-api` required for everyone.** Check import-time cost and any behavior change for users who never configure OTel. The CodSpeed report said no benchmark change, but it compared against a fallback base commit (79d42b2) and covered only 24 benchmarks.
- **Side effects in lifespan.** FastAPI may set global OTel providers during startup. It then handles races with other components, Logfire wrappers and `add_metric_reader` quirks (`_runtime.py:73-82`, `150-207`). Check:
  - the check-then-set race handling;
  - what happens with multiple `FastAPI()` apps in one process;
  - what happens to `_configured` and `_owned` state that is global and never reset;
  - duplicate exporters when another component already exports from the same environment variables. The docstring itself says it does not deduplicate (`_runtime.py:88-92`).
- **Double instrumentation.** `_legacy_otel` detects contrib's `OpenTelemetryMiddleware` by class name and module string, walking the built stack (`_asgi.py:72-84`). This is fragile. I did not read how the result suppresses native spans, so check that.
- **Hot-path edits.**
  - Every endpoint, dependency and serialization call is now inside a `with _operation(...)`.
  - Sync endpoints use a new wrapper, so check that context propagation into the threadpool still works.
  - `BackgroundTasks.__call__` is rewritten rather than delegated to Starlette.
  - The `_route_selected` calls are scattered across routing code, including the redirect branch, which re-matches the redirect scope and walks `_IncludedRouter` manually (`routing.py`, near the redirect handling). That is easy to get out of sync with the real matching logic.
- **Data capture.** `request`, `body` and `values` are stored on the telemetry object. Confirm in `_api.py` (not read) that these are not exported as attributes, since they may contain sensitive data. The ASGI code redacts a few query parameters, such as AWS and Google signatures (`_asgi.py:44-52`). That list is narrow, and I have not checked whether other query strings are redacted.
- **Exception logging.** Unhandled exceptions are emitted as OTel log records with the exception attached. Check for duplicate reporting, and whether an `HTTPException` or a validation error counts as "unhandled". The middleware's `except Exception` catches them before the handlers run.
- **Size and process.** The PR adds 5,251 lines across 36 files, mostly tests. Its description says it was AI-assisted (Codex) and then manually reviewed. The comments I could see contained only a CodSpeed bot; one other bot comment was hidden, and I saw no human review comments. The author notes that Sentry and Logfire confirmed it does not block them (2026-09-29 update).

## Uncertainty
I did not verify the span names and attributes in `_api.py` or the remaining `_asgi.py` logic, such as how `exclude` and `legacy_otel` are applied. I also did not verify the test coverage. I did not see the `pyproject.toml` patch past the first 459 characters, so the exact extras list is partial.