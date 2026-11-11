Load for Rust CPU, memory, threads, async stalls or server spikes. Use [native](native.md) for breakpoints and crash dumps.

# Rust profiling

## Build and capture

Keep the production features, optimization, allocator and workload. Add symbols at the workspace root:

```toml
[profile.release]
debug = "line-tables-only"
strip = "none"
```

Or use environment overrides:

```bash
CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_PROFILE_RELEASE_STRIP=none cargo build --release --bin <binary>
```

Use full debug information for locals; line tables suffice for source locations. Keep matching binaries, symbols and source revision with the capture. If unwinding fails, try DWARF stacks with perf or rebuild with `RUSTFLAGS="-C force-frame-pointers=yes"`; preserve existing RUSTFLAGS and disclose the changed build. Never compare a debug baseline with a release candidate.

| Question | First capture | Next evidence |
|---|---|---|
| Busy core / throughput | Native sampled stacks | Self cost, inclusive callers, repeated operation counts |
| Slow with low CPU | Request spans + every thread's stacks | Lock owner, queue depth/wait, downstream I/O |
| Many threads / uneven load | Linux `pidstat -t -p <pid> 1 <count>` or Instruments thread tracks | Runnable vs blocked, task stealing, pool saturation |
| RSS grows | RSS series + allocation profiler | Live bytes vs total allocation churn; allocator retention/mappings |
| Tokio task stalls | `tracing` spans, then Tokio Console if instrumented | Poll duration, scheduled/busy/idle states, resource waits |
| Burst / spike | Continuous short-window metrics + a capture across the event | Request IDs, queue depth, workload phase, same-window stacks |

Check installed tools and their `--help`. Do not install profilers or change host permissions implicitly. A capture window is an experiment parameter, not a timeout for normal application work.

```bash
# Attach to a verified, authorized PID. Script neither stops nor signals the target.
bash <skill>/scripts/native-capture.sh <pid> <seconds> <new-capture-directory>

# Linux, launched finite workload; no cargo/build cost in the recording.
perf record --call-graph dwarf -o <capture>/perf.data -- target/release/<binary> <args>
perf report -i <capture>/perf.data --stdio --no-children

# Installed cargo-flamegraph; use a finite workload and choose an artifact path.
cargo flamegraph --bin <binary> -o <capture>/flamegraph.svg -- <args>
```

The capture script uses macOS `sample`, Linux `perf`; it records PID/PPID, command name, duration and diagnostics, and fails visibly on permission/tool errors. It captures one PID, not a process tree. `process.txt` RSS is KiB on these hosts; `%cpu` is OS-reported, not a capture-window measurement. PID reuse is possible: verify process identity before and after attachment; discard a capture if it changed or the process exited early. Use `perf report` thread filters or Instruments tracks before attributing one thread's hotspot to every task.

On macOS, Instruments Time Profiler provides timeline/thread inspection; list installed templates with `xcrun xctrace list templates` and use its explicit time limit. Samply is an alternative native sampler with a browser-based Firefox Profiler viewer: follow installed help for headless recording and attachment, whose platform support varies. It is not Chrome DevTools.

## Memory: allocations are not RSS

On a supported Linux host with Valgrind installed:

```bash
valgrind --tool=massif --massif-out-file=<capture>/massif.out target/release/<binary> <args>
ms_print <capture>/massif.out > <capture>/massif.txt
```

Use Massif's allocation trees near the peak; use an installed massif visualizer for the time axis. Massif measures heap usage, with bookkeeping/alignment; stacks are optional, and it changes timing substantially. An allocator/custom arena may hide individual object allocations. For lower-overhead investigation consider an installed allocation profiler compatible with the actual allocator, or Instruments Allocations on macOS. Record unsupported platforms/allocators explicitly.

Distinguish total allocated bytes (churn), outstanding allocations (retention), and resident pages (RSS). Repeated equivalent work must show growth after cleanup before a leak claim. Rust ownership does not preclude retained caches, cycles in `Arc`, forgotten allocations, or unbounded queues. Inspect ownership and release paths at the measured allocation stacks; do not add manual freeing without establishing the owner.

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

Sources: [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html), [flamegraph](https://github.com/flamegraph-rs/flamegraph), [Samply](https://github.com/mstange/samply), [Massif](https://valgrind.org/docs/manual/ms-manual.html), [Tokio tracing](https://tokio.rs/tokio/topics/tracing), [Tokio Console](https://docs.rs/tokio-console/latest/tokio_console/), [tracing-chrome](https://docs.rs/tracing-chrome/latest/tracing_chrome/).
