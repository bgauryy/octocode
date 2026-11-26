Load when a Rust binary's CPU, threads or stalls must be profiled. For breakpoints use [native debugging](native.md), for crash dumps [native crashes](native-crash.md). For RSS or heap growth load [Rust memory profiling](rust-memory.md); for a stalled async task or a span timeline, [Rust async tracing and timelines](rust-async.md).

# Rust profiling

## Build and capture

For a running binary, confirm its identity and start with the PID capture below. Match its build and symbols before attributing a frame to source. For a new run, keep the production features, optimization, allocator and workload. Add symbols at the workspace root:

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

```bash
# Attach to a verified, authorized PID.
bash "$skill/scripts/native-capture.sh" "$pid" "$seconds" "$out/stacks"

# Linux, launched finite workload; no cargo/build cost in the recording.
perf record --call-graph dwarf -o "$out/perf.data" -- target/release/<binary> <args>
perf report -i "$out/perf.data" --stdio --no-children

# Installed cargo-flamegraph; use a finite workload and choose an artifact path.
cargo flamegraph --bin <binary> -o "$out/flamegraph.svg" -- <args>
```

[Capture script](profiling-platforms.md#platform-captures) output, identity checks and limits are described with the platform captures. Check thread filters or Instruments tracks before attributing a hotspot to an async task.

On macOS, Instruments Time Profiler provides timeline/thread inspection; list installed templates with `xcrun xctrace list templates` and use its explicit time limit. Samply is an alternative native sampler with a browser-based Firefox Profiler viewer: follow installed help for headless recording and attachment, whose platform support varies. It is not Chrome DevTools.

Sources: [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html), [flamegraph](https://github.com/flamegraph-rs/flamegraph), [Samply](https://github.com/mstange/samply).
