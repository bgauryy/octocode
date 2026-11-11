Load for a profiling request or unexplained CPU, memory, latency, I/O or rendering cost.

# Profiling

## Establish the workload

Record the exact command/action, runtime, build/symbols, input size, duration and machine. Separate cold startup, warm startup, idle, active work and cleanup. For an interactive program, measure the actual terminal/browser interaction: a headless run is a comparison, not proof about rendering. Browser headless mode and CLI non-interactive mode are different controls.

Save local artifacts under `<workspace>/.octocode/octocode-debugging/<capture>/`. Check available commands with `command -v`; use installed tools rather than silently installing or escalating. Profiles may contain paths, code and data. Keep raw artifacts local and read bounded summaries.

First time the unchanged workload (`time <command>` or its runner's duration), without instrumentation. For a comprehensive request, build this coverage checklist and mark unsupported dimensions explicitly:

| Dimension | Capture | What it does not prove |
|---|---|---|
| Wall time / phases | Monotonic timers, marks/measures, request timeline | CPU cost or the cause of waiting |
| CPU / functions / callers | Sampled stacks, self and inclusive top rows, flame chart | Exact invocation counts; sleeping/remote work |
| Memory / allocations | RSS, heap, external/array buffers over time; allocation stacks | One large sample is not a leak |
| Event loop / GC | Delay histogram, utilization, GC count/duration | Utilization is not process CPU utilization |
| Processes / threads | PID/PPID, process tree, per-process CPU/RSS, native stacks | Main-thread V8 samples omit native/worker CPU |
| Data / I/O | Payload sizes, file/block counters, network waterfall and spans | Block counters are not bytes or per-request latency |
| Browser / GPU | Performance trace with rendering, compositor, GPU tracks | Browser JS CPU samples are not GPU utilization |

For Rust, use [Rust profiling](rust-profiling.md). For request correlation and spike experiments, use [controlled experiments](experiments.md).

## Read the evidence

- **Self** cost is inside the function; **total/inclusive** includes callees. Inspect both: a high inclusive caller may repeat cheap work many times. Do not add overlapping inclusive rows.
- Samples approximate time and can miss short calls. For exact invocation counts or per-operation distributions, instrument the owning boundary with a counter and monotonic start/end timings; for Node, `performance.mark/measure` or `performance.timerify`. Remove temporary instrumentation afterward.
- GC near the top suggests allocation pressure; compare allocation stacks and the memory trend before calling it a leak. Compare retained memory after equivalent lifecycle/GC points, over repeated cycles. Do not force GC in the baseline unless that is the workload.
- High wall time with low CPU suggests waits, throttling, locks or I/O. Distinguish them with spans/native stacks rather than optimizing the hottest CPU function by default.

## Node and TypeScript

Create the output directory first. Use the project's TypeScript loader when required.

```bash
mkdir -p .octocode/octocode-debugging/run
node --cpu-prof --cpu-prof-dir=.octocode/octocode-debugging/run app.mjs
node <skill>/scripts/profile-top.mjs .octocode/octocode-debugging/run --top 15
node <skill>/scripts/profile-top.mjs <file.cpuprofile> --sort total --top 15
```

[profile-top.mjs](../scripts/profile-top.mjs) reads a named profile or the newest CPU/heap profile in a directory. CPU rows are sampled milliseconds; heap rows are estimated live allocated bytes. The CPU summary's “sampled busy” excludes idle pseudo-frames; a native blocking call can still appear there. Use process CPU counters to distinguish CPU work from time waiting inside such calls. `--filter src/` restricts rows, not the percentage denominator. Keep unfiltered rows too so native/runtime costs are not hidden. Open the `.cpuprofile` in DevTools Performance for call paths and the time axis; top rows alone lose ordering.

Record memory separately when possible to measure profiler overhead:

```bash
node --heap-prof --heap-prof-dir=.octocode/octocode-debugging/run app.mjs
node <skill>/scripts/profile-top.mjs <file.heapprofile> --top 15
```

`--heap-prof-interval` is bytes (default 524288); smaller intervals cost more. The profile at clean exit shows sampled allocations still live then, not all allocation churn. Use an inspector allocation-sampling window for a phase rather than claiming exit data explains a transient peak. A full heap snapshot is a different, pausing and memory-intensive operation; obtain approval for a live session and never enable near-limit snapshots casually.

### Process, memory and loop timing preload

```bash
OCTOCODE_PROFILE_DIR=.octocode/octocode-debugging/run \
  OCTOCODE_PROFILE_INTERVAL_MS=1000 \
  node --import <skill>/scripts/node-metrics.mjs app.mjs
```

The dependency-free [preload](../scripts/node-metrics.mjs) writes `<pid>-<timestamp>.metrics.jsonl`, mode 0600, and leaves stdout/stderr to the application. It does not attach to an existing process or start an inspector. Default sampling is 1000 ms; allowed interval is 10–2147483647 ms. Use a naturally terminating workload or an external timeout: the unreferenced sampler does not keep it alive or terminate it. SIGKILL/crashes may leave no final record. Disk/write/configuration errors fail visibly.

- `start` identifies runtime, PID/PPID and resolution. `sample` and `exit` contain elapsed milliseconds, cumulative CPU milliseconds since preload, memory bytes, cumulative loop/GC metrics and resource counters. `exit` includes the application's exit code.
- `memoryBytes`: RSS is process-wide; heap/external/arrayBuffers are for the main isolate. Array buffers are already included in external memory: do not sum them. Worker threads are not instrumented; inherited preloads skip them. Explicitly instrument workers or use a native profiler if they matter.
- `eventLoop`: active/idle are milliseconds, utilization is a 0–1 fraction. Delay p50/p99/max are milliseconds at 20 ms resolution, cumulative since preload; `null` means no delay samples, not zero lag. Very short/synchronous startup can occur before the loop histogram is armed.
- `gc`: observed count and total duration in milliseconds, cumulative. Exit drains pending observer records but cannot guarantee an event never delivered by the runtime.
- `resources`: Node `process.resourceUsage()` lifetime counters, not deltas since preload. CPU times here are microseconds; `maxRSS` is a lifetime high-water mark in KiB. `fsRead/fsWrite` count block I/O operations, not bytes; cached I/O and platform support can produce zeros. Subtract counters between samples for a window. CPU percent = 100 × delta(user+system CPU ms) / delta(elapsed ms), possibly over 100% across threads.

Sampling and synchronous artifact writes add overhead. Compare uninstrumented, CPU-only and metrics/memory runs; do not interpret profiler cleanup/writes as application steady-state work. To attach to an already-running process, prefer bounded OS sampling first; do not send `SIGUSR1` or open a debug port on someone else's session. If an authorized inspector is already available, bind only loopback and stop CPU/heap recording in a `finally` cleanup.

### Visualize the metrics

```bash
node <skill>/scripts/metrics-report.mjs <capture.metrics.jsonl> <new-report.html>
```

The offline HTML shows shared-time CPU, RSS, main-isolate heap/external memory, interval GC duration and cumulative loop maximum, with every measurement in a table. No external assets or uploads. Input is one preload capture; malformed/incomplete JSONL fails visibly. CPU uses counter deltas divided by actual elapsed time and may exceed 100%. Each interval hides shorter bursts; the cumulative loop maximum cannot locate a later spike. Keep the raw JSONL and label interpolated lines. Pair a suspect interval with CPU stacks, not just the largest chart value.

## Process tree and native stacks

Inspect PID, PPID, command name, CPU and RSS without dumping credentials from arguments or environment. RSS units vary by platform. Identify owned descendants before attributing CPU/memory; separate the agent, language servers, browser and workload. Process totals can double-count shared memory, and parent CPU counters generally exclude children.

- macOS: `ps -p <pid> -o pid=,ppid=,rss=,%cpu=,etime=,comm=`; `sample <pid> 5 -file <capture>/stacks.txt` records native/thread stacks. Use `top -l 3 -s 1 -pid <pid> -stats pid,command,cpu,mem,threads,time` for bounded observations. The first CPU observation may cover a different interval; prefer subsequent ones.
- Linux: installed `pidstat -p <pid> -r -u -d 1 5`; `perf record -g -o <capture>/perf.data -- <command>`, then `perf report --stdio --no-children`. Permission denial is a gap, not a reason to use sudo without approval.
- Native optimized builds need symbols: Rust release `debug = true`, C/C++ `-g` with production optimization. Use `valgrind --tool=massif` for native heap growth when installed; this heavily changes timing.

## Data and I/O

Measure bytes/items at the input/output boundary and duration at each await, queue, serialization and storage stage. Record payload size and throughput alongside p50/p95/p99 latency with sample count. Do not log secret payloads. Use a network waterfall or request spans to separate DNS/connect/TLS, server wait and transfer; remote model latency is not local CPU.

On macOS, installed `fs_usage` or Instruments File Activity can reveal file paths/waits but may need privileges and expose data; request approval rather than running sudo. On Linux, installed `strace -f -c -- <command>` gives syscall counts/time with substantial overhead. Aggregate host disk tools cannot attribute I/O to this process alone. Unsupported/zero block counters do not establish absence of I/O.

## Browser and GPU

Route live browser capture through `octocode-chrome-devtools` and a browser agent. Record the same visible/headless setting, viewport, throttling, interaction and input. Capture a bounded Performance trace with JS stacks, rendering/layout/paint and network. Inspect long tasks and their callers, frame timing and compositor/GPU tracks where present. Trace screenshots and request bodies can contain private data.

On macOS, check `xcrun --find xctrace`, then `xcrun xctrace list templates`; choose an installed Time Profiler, Allocations, Metal or GPU template and set a time limit. On other hosts, check the installed GPU vendor profiler and its permissions. A terminal app often delegates rendering to the terminal emulator/compositor: profiling Node alone does not measure that GPU work. Missing tools/tracks mean **GPU not measured**, not zero GPU cost. Do not install Xcode or collect system-wide traces without authorization.

## Python and Go

```bash
python3 -m cProfile -o .octocode/octocode-debugging/run/run.prof app/main.py
python3 -c "import pstats; pstats.Stats('.octocode/octocode-debugging/run/run.prof').sort_stats('cumtime').print_stats(20)"
go test -cpuprofile cpu.out -memprofile mem.out -bench .
go tool pprof -top cpu.out
```

Python `tottime` is self, `cumtime` inclusive. `tracemalloc` snapshots compared over repeated operations locate Python allocation growth, not native/GPU memory. With py-spy installed, record a bounded workload to a flame graph; avoid interactive `top` in a batch shell. Go pprof distinguishes allocation space from in-use space; select the intended sample index. Both runtimes still need OS/process and I/O evidence for a comprehensive profile.

## Verify and report

Test the strongest rival: is the hot path startup only, instrumentation overhead, a fixture, I/O waiting, or actual repeated user work? Repeat identical uninstrumented runs and compare medians and spread; use enough operations for tail percentiles. No single run proves a speedup. Report workload/mode, artifact paths, duration/sample count, units, top self/inclusive stacks, memory trend, timing/process/I/O evidence, coverage gaps and the next targeted experiment. Do not manufacture a bottleneck when the session is idle.

## Sources

- [Node CPU/heap command-line profiling](https://nodejs.org/docs/latest-v24.x/api/cli.html)
- [Node performance hooks](https://nodejs.org/docs/latest-v24.x/api/perf_hooks.html) and [process metrics](https://nodejs.org/docs/latest-v24.x/api/process.html)
- [Node inspector and its security constraints](https://nodejs.org/docs/latest-v24.x/api/inspector.html)
- [Chrome Performance reference](https://developer.chrome.com/docs/devtools/performance/reference)
- [Apple Instruments](https://developer.apple.com/tutorials/instruments)
