PR #16403 is merged (merge commit `4b3949cd`). It adds built-in OpenTelemetry to `FastAPI()`. It is on by default, adds a hard dependency, and can install exporters at lifespan startup.

I read the diffs for `applications.py`, `routing.py`, `background.py` and `pyproject.toml`, and the full `fastapi/telemetry/_runtime.py`. I did not read `telemetry/_asgi.py` (446 lines) or `telemetry/_api.py` (276 lines). I also did not read the tests or docs, and I saw only the first part of the `pyproject.toml` patch. Anything I say about span or metric contents is therefore not verified.

## Runtime behavior changes

**1. Telemetry is on by default.**
- `FastAPI.__init__` takes a new `telemetry: TelemetryConfig | None` argument.
- The defaults are tracing, metrics, logs and `operation_spans` all `True`, `auto_configure=True`, and the providers `None`, meaning the global ones (`applications.py`, ~L1030).
- `__call__` routes http/websocket scopes through `NativeTelemetry` when it is enabled. If the scope already has a `"fastapi.telemetry"` key, or telemetry is disabled, it falls back to the plain Starlette path.
- Before delegating, it builds `middleware_stack` eagerly if it is `None`. It also calls `_legacy_otel(self.middleware_stack)`, which presumably detects the contrib instrumentation middleware and avoids double instrumentation. I did not confirm that.

**2. New middleware order.** `ExceptionTelemetryMiddleware` is inserted right after `ServerErrorMiddleware` and before user middleware. Every app's middleware stack changes, whether or not it uses OpenTelemetry.

**3. Extra work on the request path.** New `_operation(name=...)` context managers wrap these steps:
- endpoint execution (`routing.py`, `run_endpoint_function`)
- dependency solving
- response serialization
- websocket dependencies and endpoint
- each background task (`BackgroundTasks.__call__` is overridden in `background.py`)

Sync endpoints now go through `_run_sync_endpoint` inside `run_in_threadpool`, instead of calling the function directly.

**4. Data captured on a per-request telemetry object.** `request`, `body`, solved `values` and `errors`, and `websocket` are stored on it. `_validation_failed(...)` is called before validation errors are raised. `_route_selected(...)` is called at about seven routing sites, including redirect-slash handling, low-priority matches, mounts and included routers. The redirect branch replicates the `_IncludedRouter._match` walk.

**5. Lifespan wrapper and environment auto-configuration** (`_runtime.py`).
- On `lifespan.startup`, `_configure_from_environment` adds OTLP exporters from `OTEL_*` variables.
- It runs only if `auto_configure` is on and `OTEL_SDK_DISABLED` is not `true`.
- If no global provider is configured, it creates and installs an SDK provider with `shutdown_on_exit=False`. Otherwise it attaches its own processor or reader to the existing provider.
- Flush happens on shutdown and startup-failed lifespan messages, via `run_sync`. There is also an `atexit` shutdown hook.

**6. Dependencies** (`pyproject.toml`).
- `opentelemetry-api>=1.44.0` becomes a hard dependency for every FastAPI install.
- A new `opentelemetry` extra adds the SDK and the OTLP http exporter, and `standard` gets the same.

## What a reviewer should watch

- **Default-on with a hard dependency.** Everyone pays for `_operation` wrappers and the new middleware. Check the overhead when no SDK is installed, since the API should be a no-op. There is no benchmark in what I read.
- **Startup can now fail.** `_configure_from_environment` raises `FastAPIError`, then sends `lifespan.startup.failed`, in three cases:
  - an exporter other than `otlp` or `none`
  - a protocol other than `http/protobuf`
  - the SDK packages missing
  
  An app that was fine before could refuse to start if it has `OTEL_EXPORTER_OTLP_ENDPOINT` set and no extra installed. That is a behavior break to weigh. Note also that `OTEL_TRACES_EXPORTER=console` or `grpc` now raises instead of being ignored.
- **Coexistence with other OpenTelemetry setups.**
  - Contrib `FastAPIInstrumentor` and `opentelemetry-instrument` auto-instrumentation could double-instrument or double-export. `_legacy_otel` handles only the first case, as far as I saw.
  - FastAPI "does not inspect or deduplicate other components' exporters" (its own docstring). That is a duplicate-export risk with no automatic guard.
  - The `ProxyMeterProvider` special case matches a Logfire class by module and name string. That is brittle.
- **Global state and races.** Module-level `_owned` and `_configured` lists, an `RLock` and `atexit` are all used. Multiple apps in one process, test isolation, and the "another component configured the global provider concurrently" branch (`provider.shutdown()` then re-attach) deserve scrutiny.
- **`_flush` is called without the lock.** It iterates a snapshot of `_owned`, while `_shutdown` takes the lock. Compare the two.
- **Lifespan behavior.** Configuration happens inside `wrapped_receive`, on receiving startup. `lifespan.shutdown.*` messages now trigger a synchronous `force_flush` in a thread. That can delay shutdown if the collector is unreachable, and I saw no timeout.
- **Data exposure.** The request body and solved dependency values are retained on a telemetry object for the request's life. Check what `_api.py` and `_asgi.py` actually put in span attributes or logs. This is a PII risk, and I did not verify it.
- **Routing changes are invasive.** `_route_selected` is scattered through `routing.py`, including a redirect branch that re-walks `_IncludedRouter` matches. Check that it has no side effects on match results and is cheap when telemetry is off.
- **Size and process.**
  - The PR is +5251/−46 across 36 files, and the description says it was AI-assisted.
  - The follow-up commit (the file was last modified 2026-09-29) and the author's note that Sentry and Logfire were consulted suggest the API was still moving. It was merged three days after opening.
  - The author says internals will be refactored soon, so the private hooks (`_operation`, `_route_selected`) may change.