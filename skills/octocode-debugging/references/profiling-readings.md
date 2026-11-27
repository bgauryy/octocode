Load when choosing which counter or stack to capture next, or when a recording contradicts what the runtime should show.

# Choose and read a measurement

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
| **Rust / Tokio** · An async span appears busy throughout an await | An entered span guard held across suspension attributes unrelated work on that worker to the span: [instrument the future](rust-async.md). A poll can itself block. Use thread CPU samples to establish CPU cost. [Tracing spans](https://docs.rs/tracing/latest/tracing/span/index.html) |
| **Native code** · Caller chains stop early or contain implausible frames | An optimized binary may omit frame pointers. Matching symbols resolve names. Recovering callers also requires a compatible unwinding method, such as DWARF call graphs on supported Linux builds. [perf call graphs](https://man7.org/linux/man-pages/man1/perf-record.1.html) |
| **JVM** · JFR shows few lock or I/O events during a slow operation | Check the recording’s enabled events and duration thresholds. Many short waits can contribute latency while each falls below the threshold. [JFR performance diagnostics](https://docs.oracle.com/en/java/javase/25/troubleshoot/troubleshoot-performance-issues-using-jfr.html) |
| **.NET** · Events disappear during a burst | EventPipe can drop events when its buffer fills. Check the capture diagnostics and enabled providers before interpreting the gap as idle time. A narrower provider selection can reduce event loss. [dotnet-trace buffering](https://learn.microsoft.com/en-us/dotnet/core/diagnostics/dotnet-trace) |
| **Browser** · A long task has no JavaScript caller frames | DevTools can record high-level events with JS sampling disabled. Check the capture settings before attributing the task to layout or paint. [Chrome Performance settings](https://developer.chrome.com/docs/devtools/performance/reference#disable-js-samples) |
| **GPU** · A host API call lasts microseconds while the frame takes milliseconds | Submission returns while work is queued: follow it to device execution and the completion the frame waits for. [GPU timelines](gpu.md) |
| **Full stack** · Summed spans exceed request latency | Spans nest, overlap and repeat across retries: follow the dependency that gates completion. [Request correlation](experiments.md#follow-a-full-stack-request) |
