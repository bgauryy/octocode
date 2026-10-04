**Short answer:** PR #16403 is merged (merge commit `4b3949cd`, 36 files, +5251/−46). It adds built-in OpenTelemetry tracing, metrics and logs to every `FastAPI` app. It is on by default, but it only does work when an OTel provider is configured. It also adds a hard dependency, new middleware, and wrappers around the request path.

I read the diffs for `applications.py`, `background.py`, `routing.py` and `pyproject.toml`, plus `_runtime.py` and the top of `_asgi.py` at head `daba280d`. I did not read `_api.py` or the rest of `_asgi.py` (span/metric naming, `finish`, send/receive wrapping) in full. I did not read the tests or docs, and I did not run anything.

## Runtime behavior changes

1. **New constructor option.** `FastAPI(telemetry=TelemetryConfig)` is merged over defaults: tracing, metrics, logs and operation_spans on, `auto_configure=True`, and `exclude=None` (`fastapi/applications.py` diff, `__init__`).

2. **`FastAPI.__call__` is rewritten** (`applications.py`):
   - Lifespan scopes go through `telemetry._runtime.lifespan`.
   - HTTP and websocket requests are wrapped by `NativeTelemetry`, but only if `_native_telemetry.enabled()` is true. That requires some provider to be configured, either explicitly or globally (`_asgi.py:172-196`).
   - The middleware stack is now built eagerly inside `__call__` (`if self.middleware_stack is None: ...`). It also sets `scope["app"] = self`.

3. **New middleware.** `ExceptionTelemetryMiddleware` is always inserted right after `ServerErrorMiddleware` in `build_middleware_stack`, even when telemetry is off.

4. **Extra spans around request handling** (`routing.py`, `background.py`), through `_operation(...)`:
   - `endpoint`, `dependencies`, `serialization` and `background_task`.
   - Sync endpoints now run via `run_in_threadpool(_run_sync_endpoint, function=..., arguments=...)` instead of calling the endpoint directly.
   - `BackgroundTasks.__call__` is overridden to run each task inside a span.
   - Websocket handlers get the same `dependencies` and `endpoint` spans.

5. **Per-request data capture.** The request, body, resolved values and errors are stored on the telemetry data object (`telemetry_data.request/body/values/errors`). `_validation_failed(...)` is called before validation errors are raised. `_route_selected(...)` is called in about six places: included-router dispatch, mounts, partial matches, the redirect-slashes path, and low-priority matches. This is how the route template ends up on the span and metrics.

6. **Coexistence with the contrib instrumentation.** `_legacy_otel(stack)` walks the middleware stack. If it finds `opentelemetry.instrumentation.asgi.OpenTelemetryMiddleware`, native tracing, logs and metrics are turned off to avoid double-instrumenting (`_asgi.py:72-84`, `235-244`). `exclude(scope)` skips a request and sets `scope["fastapi.telemetry"]=None` so mounted FastAPI apps also skip it.

7. **Auto-configuration at lifespan startup** (`_runtime.py:85-207`):
   - If `OTEL_{TRACES,METRICS,LOGS}_EXPORTER` is `otlp` (the default) and an OTLP endpoint env var is set, FastAPI creates OTLP exporters.
   - If the global provider is unconfigured, it installs its own SDK provider with `set_*_provider`. Otherwise it adds a processor or reader to the existing provider.
   - It registers an `atexit` shutdown, and flushes on lifespan shutdown or failure via a thread (`_runtime.py:39`, `226-237`).
   - It raises `FastAPIError` for non-OTLP exporters, non-`http/protobuf` protocols, bad URLs, or missing SDK packages. The lifespan wrapper turns that into `lifespan.startup.failed`.
   - It is skipped if `OTEL_SDK_DISABLED=true` or `auto_configure=False`.

8. **Dependencies** (`pyproject.toml`):
   - `opentelemetry-api>=1.44.0` becomes a **core** dependency.
   - SDK and OTLP-http exporter are added to a new `opentelemetry` extra, and to the `standard`, `standard-no-fastapi-cloud-cli` and `all` extras.
   - The tests group gains logfire, sentry-sdk and opentelemetry-instrumentation-fastapi.

## What a reviewer should watch

- **Hot-path cost with telemetry disabled.** `ExceptionTelemetryMiddleware` is always installed. `enabled()` is evaluated on every request and calls `_unconfigured()` on up to three global providers. `_operation` and `get_telemetry_data` wrap the endpoint, dependency and serialization steps on every request. Ask for a benchmark with telemetry off.
- **Hidden global side effects.** Startup can call `trace.set_tracer_provider` and the metrics and logs equivalents, and can attach exporters to someone else's provider. It can also leave an `atexit` hook. This is triggered purely by `OTEL_EXPORTER_OTLP_*` env vars, so setting those env vars for another tool can now make FastAPI export data. The code comments admit it doesn't deduplicate exporters, so double export is possible when something else also reads the same env vars.
- **Startup can now fail.** A misconfigured env var (for example `OTEL_TRACES_EXPORTER=jaeger`, or grpc protocol) makes app startup fail with `FastAPIError`, when it previously worked. Check this against the default `standard` install.
- **Vendor-specific hacks.** `_registration_provider` special-cases the Logfire `ProxyMeterProvider` by module and class name (`_runtime.py:73-82`). `_legacy_otel` matches the contrib middleware by module and class name string. Both are brittle against upstream renames.
- **Concurrency and race handling.** The "another component configured the provider concurrently" branch (`_runtime.py:172-180`) uses a lock only within FastAPI. Check this path has a test.
- **Routing edits touch many branches.** `_route_selected` is added in six places, including a re-match loop for redirect_slashes (`routing.py` diff, near `app`). A missed branch would give wrong or missing `http.route` values. Look for coverage of mounts, included routers, and 404 and redirect cases.
- **Behavior changes unrelated to telemetry.** `__call__` now builds the middleware stack eagerly and sets `scope["app"]`. `BackgroundTasks.__call__` is overridden. Sync endpoints go through a new wrapper in the threadpool. Check whether these affect contextvars propagation, subclasses that override `__call__`, and anything that relied on `Starlette.__call__` semantics.
- **Sensitive data.** The request body and resolved values are captured on the telemetry data object for integrations. I did not verify what ends up in span attributes or logs, so check `_asgi.py` and `_api.py` for PII or body leakage.
- **Size and provenance.** The PR is about 5k lines, mostly tests, and the description says it was written with AI help and manually reviewed. The author also says Sentry and Logfire have checked that it doesn't block them. It also adds no OTel contrib change, and the follow-up to move contrib onto the built-in integration is not done.
- **Dependency footprint.** A core dependency on `opentelemetry-api>=1.44.0` affects every install, including users who don't want OTel. Version pinning against other OTel packages in the same environment is worth checking.

## Uncertainty
I did not verify the span names, attribute conventions, or whether the semantic conventions are stable or opt-in. I also did not check the "no-op when no provider" claim beyond the `enabled()` logic. Whether the PR was changed after review is something I didn't check.