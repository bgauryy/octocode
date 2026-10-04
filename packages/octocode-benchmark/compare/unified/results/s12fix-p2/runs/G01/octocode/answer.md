**Verdict.** PR #16403 is merged (2026-09-29, target `master`). It adds built-in OpenTelemetry traces, metrics and logs to `FastAPI`. The feature is on by default but only does work once an OTel provider exists. I read the diff at head `daba280d` and the merged `fastapi/telemetry/_runtime.py` and `_asgi.py` at `4b3949cd`. I did not read the 36-file diff in full: the `_asgi.py` span/metric internals, `_api.py` after about 4.3k of its 10k characters, `background.py` (the diff was cut off), and all of `tests/test_telemetry/*`.

## Runtime behavior changes

**New dependency and API**
- `opentelemetry-api>=1.44.0` becomes a hard dependency of every install (`pyproject.toml:50`).
- The SDK and OTLP/HTTP exporter go into the `opentelemetry`, `standard`, `standard-no-fastapi-cloud-cli` and `all` extras.
- `FastAPI(telemetry=TelemetryConfig)` is new. Its keys are:
  - providers: `tracer_provider`, `meter_provider`, `logger_provider`
  - toggles: `tracing`, `metrics`, `logs`, `operation_spans`, `auto_configure`
  - filter: `exclude`
- Everything defaults to on (`fastapi/applications.py`, the `self._telemetry` dict).

**`FastAPI.__call__` is rewritten** (`fastapi/applications.py`)
- Lifespan scopes now go through `fastapi.telemetry._runtime.lifespan`.
- HTTP and websocket scopes go through `NativeTelemetry` only if `enabled()` is true. That requires at least one signal turned on and either an explicit provider or a non-default global provider. Otherwise it falls through to plain `super().__call__`.
- `build_middleware_stack` now inserts `ExceptionTelemetryMiddleware` right after `ServerErrorMiddleware`. It records exceptions before the error handlers turn them into responses.

**Environment-driven auto-configuration** (`_runtime.py:85-207`)
- It runs on `lifespan.startup`. If `OTEL_EXPORTER_OTLP_*ENDPOINT` is set and no `OTEL_SDK_DISABLED=true`, FastAPI creates or extends the global providers.
- A global provider that is still unconfigured is replaced with a new SDK provider (`trace.set_tracer_provider` and the metrics/logs equivalents).
- If a provider already exists, FastAPI adds its own OTLP processor or reader to it.
- Only the `otlp` exporter and the `http/protobuf` protocol are accepted. Anything else raises `FastAPIError`, which is sent as `lifespan.startup.failed` (`_runtime.py:52-64, 220-223`).
- Flushes and shutdown go through `atexit` and the lifespan shutdown path, but only for components FastAPI created itself.

**Request-path instrumentation** (`fastapi/routing.py`)
- Each request gets `_operation` spans named `dependencies`, `endpoint` and `serialization`. Background tasks get a span too, via a new `BackgroundTasks.__call__` override in `fastapi/background.py`. The websocket handler gets `dependencies` and `endpoint` spans.
- Sync endpoints now run through `_run_sync_endpoint` in the threadpool instead of directly through `dependant.call`.
- `_route_selected(...)` is called at each routing step, including mounts, redirects, frontend routes and the low-priority fallback. This is how `http.route` gets set.
- A `TelemetryData` object holds `request`, `websocket`, `body`, `values` and `errors`. It exists so Logfire and Sentry can read data without monkeypatching.
- Validation failures are logged as warnings through `_validation_failed`.

## What a reviewer should watch

1. **Default-on and hard-dep surface.** The base install now pulls `opentelemetry-api`, and every request goes through the new code path as soon as any provider is configured. Check that the disabled path really is a no-op.
2. **`routing.py` hot path.** It adds many `_route_selected` call sites. The redirect branch re-runs `_match` in a loop over `_IncludedRouter`s, but only when `scope["fastapi.telemetry"]` is set. Check for missed or incorrect `http.route` values and for double matching.
3. **Sensitive data.**
   - `TelemetryData.errors` contains the original input values, and `body` is the raw body.
   - The docs say exception logs include type, message and stack trace, which can hold sensitive data. They are on by default and are recorded even for unsampled traces.
   - The code marks `TelemetryData` as read-only for integrations, but nothing enforces that.
4. **Global-state side effects.**
   - FastAPI sets global providers, mutates `_owned` and `_configured` under a module-level lock, and registers an `atexit` hook.
   - It deduplicates only its own registrations. It does not inspect other components' exporters, so enabling both FastAPI auto-config and another library's env export can duplicate exports. The docs tell users to set `auto_configure=False` in that case.
   - The docs also warn that independent telemetry for mounted sub-apps is not guaranteed.
5. **Startup failures.** A bad `OTEL_*` env var (non-OTLP exporter, non-protobuf protocol, invalid URL, or a missing SDK extra) now fails app startup. That is a behavior change for anyone with those variables already set for other tooling.
6. **Compatibility hacks.**
   - `_legacy_otel` detects `opentelemetry.instrumentation.asgi.OpenTelemetryMiddleware` by module and class name.
   - `_registration_provider` special-cases `logfire._internal.metrics.ProxyMeterProvider` (`_runtime.py:73-82`).
   - Both are fragile against vendor refactors.
6. **Process.** The PR body says it was written with AI assistance and manually reviewed. The author says Sentry and Logfire were checked and are not blocked. They suggest a follow-up so OTel contrib uses the built-in integration. Until then, contrib's instrumentation plus native telemetry could double-span. `_legacy_otel` is the guard, so verify it in `test_integrations.py`.
7. **Testing.** The tests are subprocess-based, and the `tests` dependency group now includes `logfire`, `sentry-sdk` and `opentelemetry-instrumentation-fastapi`. I didn't read them, so I can't say how well they cover the points above.

**Uncertainty:** I did not read the span, metric and semantic-convention logic in `_asgi.py` or `_api.py`, or the `background.py` change in full.