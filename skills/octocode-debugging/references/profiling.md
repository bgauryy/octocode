# Profile a running workload

## Identifying a live target

When attaching by PID, verify the instance from the profiler’s PID namespace. For example, on Linux or macOS, `ps -p "$pid" -o pid=,ppid=,lstart=,comm=` shows the parent, start time and executable. A reused PID or a launcher that delegates to workers can lead you to the wrong process. An OS thread ID identifies a thread. A runtime task or span ID can follow work that migrates between threads.

For a timing question, an existing request ID, frame number or phase marker can locate the relevant window. Monotonic clocks measure elapsed time within their clock domain. If you need a comparison, consider which differences in warmup, workload or concurrency could account for the result.

## Pick the next measurement

| Question | Measure in the selected window | How to read it |
|---|---|---|
| Which resource limits progress? | Completed operations/s and latency alongside resource utilization, queue depth or throttling, and errors | Look at the busiest worker, core or device. An average across the host can hide a saturated worker. |
| Who consumed CPU? | Per-process and per-thread user + system CPU deltas, then on-CPU call stacks | CPU % = 100 × ΔCPU time / Δelapsed in the same units. Here 100% is one core. State the tool's denominator. Parent counters generally exclude children. |
| Why was runnable work delayed? | Scheduler wakeup → scheduled-in delay, runnable queue and container CPU quota/throttling | Separate time waiting for CPU from time asleep on a lock or I/O. Check the workload's cgroup and available cores. |
| Where did elapsed time go? | Queue, lock, I/O and dependency spans, plus stacks from the stalled interval | Find who can satisfy the wait. Repeated identical idle stacks alone do not establish a deadlock. |
| Did memory grow? | RSS, managed/native live bytes and allocation sites across equivalent cycles | Compare survivors after cleanup. RSS includes mappings and allocator reserves. Summing worker RSS can double-count shared pages. |
| Is it disk, network or storage? | Bytes and operations at the boundary, queue depth, service/wait duration, retries | Separate connection-pool wait, DNS/connect/TLS, first byte and transfer. Cached I/O can leave block counters flat. Confirm remote time with a downstream span. |

For tail latency, report the number of completed operations, errors/timeouts and the distribution by phase. A p99 from a handful of requests has little resolution.

For sampled stacks, **self** is weight in the leaf and **inclusive** includes its callees. Read both on the same interval. Inclusive rows overlap. Count calls explicitly when frequency matters. Check for lost events, missing symbols, empty windows and profiler overhead before interpreting differences. A flame graph groups stack weights. A flame chart or trace preserves event order. [USE method](https://www.brendangregg.com/usemethod.html) · [Off-CPU analysis](https://www.brendangregg.com/offcpuanalysis.html) · [Flame graphs](https://www.brendangregg.com/flamegraphs.html)

## Profiling gotchas by runtime

| When the recording looks surprising | What can explain it and how to tell |
|---|---|
| **Python** · A function has thousands of “hits” | Check which profiler wrote the file. cProfile counts calls. A statistical sampler counts observations and can see one long call many times. Both can write pstats. [Python profiling](python-profiling.md) |
| **Node.js** · Event-loop utilization is near 100% while CPU is quiet | A synchronous wait, such as waiting for a child process, can keep the loop occupied. Compare process CPU deltas over the same window and inspect the blocking frame. Worker isolates have their own loops. [Node performance hooks](https://nodejs.org/api/perf_hooks.html) |
| **Go** · Mutex contention exceeds the recording’s wall time | Several goroutines can wait on one holder concurrently. The mutex profile charges their accumulated delay to the holder’s unlock stack. A goroutine dump or execution trace can identify the waiters. [Go profile semantics](https://pkg.go.dev/runtime/pprof) |
| **Rust / Tokio** · An async span appears busy throughout an await | An entered span guard held across suspension can attribute unrelated work on that worker to the span. Instrumenting the future scopes span entry to its polls. A poll can itself block. Use thread CPU samples to establish CPU cost. [Tracing spans](https://docs.rs/tracing/latest/tracing/span/index.html) |
| **Native code** · Caller chains stop early or contain implausible frames | An optimized binary may omit frame pointers. Matching symbols resolve names. Recovering callers also requires a compatible unwinding method, such as DWARF call graphs on supported Linux builds. [perf call graphs](https://man7.org/linux/man-pages/man1/perf-record.1.html) |
| **JVM** · JFR shows few lock or I/O events during a slow operation | Check the recording’s enabled events and duration thresholds. Many short waits can contribute latency while each falls below the threshold. [JFR performance diagnostics](https://docs.oracle.com/en/java/javase/25/troubleshoot/troubleshoot-performance-issues-using-jfr.html) |
| **.NET** · Events disappear during a burst | EventPipe can drop events when its buffer fills. Check the capture diagnostics and enabled providers before interpreting the gap as idle time. A narrower provider selection can reduce event loss. [dotnet-trace buffering](https://learn.microsoft.com/en-us/dotnet/core/diagnostics/dotnet-trace) |
| **Browser** · A long task has no JavaScript caller frames | DevTools can record high-level events with JS sampling disabled. Check the capture settings before attributing the task to layout or paint. [Chrome Performance settings](https://developer.chrome.com/docs/devtools/performance/reference#disable-js-samples) |
| **GPU** · A host API call lasts microseconds while the frame takes milliseconds | Submission can return while work is queued. Follow the submission to the device queue, transfers, execution and the completion the frame waits for. Overlapping kernels can add up to more time than the frame took. [GPU timelines](gpu.md) |
| **Full stack** · Summed spans exceed request latency | Parents include child time, parallel calls overlap and retries can share a trace ID. Follow the dependency that gates completion. Cross-host timestamp subtraction requires aligned clocks. [Request correlation](experiments.md#follow-a-full-stack-request) |

## Platform captures

In examples, set `pid` to the verified target, `seconds` to the observation window, `out` to an existing capture directory and `skill` to this skill's absolute path. Check installed tool help for supported events and permissions. Preserve the raw recording and capture diagnostics.

| Platform | Live target | Launch or inspect |
|---|---|---|
| macOS | `bash "$skill/scripts/native-capture.sh" "$pid" "$seconds" "$out/stacks"` takes periodic native thread stacks, including threads at rest. Pair these with CPU deltas. | Use Instruments Time Profiler on a launched app for CPU attribution. Inspect thread and time range. Use System Trace for scheduling if available. |
| Linux | `pidstat -t -u -w -p "$pid" 1 5` shows threads, CPU and context switches. The same [capture script](../scripts/native-capture.sh) uses perf on a PID. Check its event and process coverage. A host CPU average can hide a worker starved by its cgroup quota. | `perf record --call-graph dwarf -o "$out/perf.data" -- ./app` then `perf report --stdio -i "$out/perf.data"`. For scheduler latency use a scheduling trace. For cgroup v2 check deltas in the target's `cpu.stat`. |
| Windows | Select the relevant WPR profile and record the affected interval. System-wide ETW collection can include other processes. | Stop and save the ETL, then filter by process, thread and interval in WPA. CPU Usage (Sampled) shows stacks. With scheduling events, CPU Usage (Precise) can distinguish a thread ready for CPU from one blocked on a dependency. |

[Linux perf record](https://man7.org/linux/man-pages/man1/perf-record.1.html) · [pidstat](https://man7.org/linux/man-pages/man1/pidstat.1.html) · [Linux cgroup v2](https://docs.kernel.org/admin-guide/cgroup-v2.html) · [Apple Instruments](https://developer.apple.com/tutorials/instruments) · [Windows Performance Recorder](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/windows-performance-recorder) · [WPA CPU analysis](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/cpu-analysis)

## Other managed runtimes

- **JVM.** For a live JVM with JDK diagnostic access, check `jcmd "$pid" help JFR.start`, then `jcmd "$pid" JFR.start name=investigation settings=profile duration=30s filename="$out/run.jfr"` using an absolute path writable by the target. For a launch, put `-XX:StartFlightRecording=settings=profile,filename=run.jfr,dumponexit=true` before the application arguments. Inspect the recording with JDK Mission Control or `jfr summary run.jfr`. Compare execution samples with GC, parks and monitor events enabled in that recording. [JDK diagnostic commands](https://docs.oracle.com/en/java/javase/25/docs/specs/man/jcmd.html) · [JFR tool](https://docs.oracle.com/en/java/javase/25/docs/specs/man/jfr.html)
- **.NET.** With dotnet-trace installed, use `dotnet-trace collect -p "$pid" --duration 00:00:00:10 -o "$out/run.nettrace"`, or `dotnet-trace collect -o "$out/run.nettrace" -- dotnet App.dll` for a finite launch. Check the tool's profiles/providers before capture. Inspect sampled managed stacks and runtime events in the trace. For a hang, `dotnet-stack report -p "$pid"` gives a point-in-time managed stack for each thread. [dotnet-trace](https://learn.microsoft.com/en-us/dotnet/core/diagnostics/dotnet-trace) · [dotnet-stack](https://learn.microsoft.com/en-us/dotnet/core/diagnostics/dotnet-stack)
- **Other runtimes.** OS counters and stacks still locate the busy or waiting process. Use the runtime's supported sampler or request spans when native symbols stop at a VM, JIT or executor. Label what remains opaque.
