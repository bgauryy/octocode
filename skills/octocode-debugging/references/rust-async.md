Load when a Rust async task stalls or waits, or a span timeline in Chrome trace format is needed.

# Rust async tracing and timelines

## Async and Chrome-format timelines

Native stacks show OS threads, not the lifetime of suspended futures. Add `tracing` spans at request, queue, lock, I/O and CPU boundaries, with IDs and payload sizes rather than payload contents. Use `#[tracing::instrument(skip_all, fields(request_id = %id))]` where applicable. Use `.instrument(span)` for futures; do not hold a `span.enter()` guard across `.await` (it can attribute other tasks to the wrong span).

Tokio Console requires application instrumentation via `console-subscriber` and its documented Tokio feature/configuration setup; it cannot attach to an arbitrary uninstrumented Rust PID. Keep telemetry local. Its interactive UI requires an interactive session; use saved spans for unattended reports.

For a Chrome-format timeline, use an opt-in `tracing-chrome` layer with the existing subscriber (do not initialize a second global subscriber):

```rust
use tracing_subscriber::prelude::*;
let (layer, guard) = tracing_chrome::ChromeLayerBuilder::new()
    .file("capture/trace.json")
    .include_args(true) // only with audited, non-sensitive span fields
    .build();
tracing_subscriber::registry().with(layer).init(); // standalone example only
// Run the finite workload with tracing spans.
drop(guard); // flush before reading the trace
```

Create the output directory first. `include_args` defaults to false; enable it for request-ID correlation only after auditing existing span fields, since it exports their values too. Keep tracing dependencies/configuration opt-in and retain the guard until capture ends.

View the exported trace in Perfetto or the Chrome trace viewer. This visualizes emitted spans, not automatic native CPU samples. A span's elapsed lifetime includes waits; async task migration needs correct instrumentation and interpretation. Chrome's Node inspector does not attach to native Rust processes.

Sources: [Tokio tracing](https://tokio.rs/tokio/topics/tracing), [Tokio Console](https://docs.rs/tokio-console/latest/tokio_console/), [tracing-chrome](https://docs.rs/tracing-chrome/latest/tracing_chrome/).
