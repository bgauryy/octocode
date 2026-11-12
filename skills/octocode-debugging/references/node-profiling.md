# Node.js profiling

## Already running

Identify the worker handling the request. For an existing private inspector, [attach DevTools to that Node target](node.md#attach-with-chrome-devtools). Start a Performance recording before the operation, stop after it and save the profile. Select the affected interval in Bottom-up and Call tree. If the inspector is unavailable, native sampling from the PID can still show worker, addon or runtime activity. Report which isolates, workers and child processes the capture covers. [Chrome Node Performance](https://developer.chrome.com/docs/devtools/performance/nodejs)

## Launch

Use the production entry point and TypeScript loader, and a fresh `out` directory per run. Choose the run that answers the question.

```bash
node --cpu-prof --cpu-prof-dir="$out" app.mjs
node "$skill/scripts/profile-top.mjs" "$out" --sort self
node "$skill/scripts/profile-top.mjs" "$out" --sort total
```

The [profile summary](../scripts/profile-top.mjs) accepts a named profile or selects the newest CPU/heap profile in a directory. With multiple profiles, pass the file for the intended isolate. CPU weights are sampled milliseconds. The summary excludes idle pseudo-frames from its busy denominator. A native blocking call can still appear there. Compare process CPU deltas and inspect the raw profile's time axis before attributing a delay to that function.

For allocation sites, use `node --heap-prof --heap-prof-dir="$out" app.mjs` on a separate run. Inspect the resulting `.heapprofile` with the same script. It estimates bytes sampled and still live at exit. For transient churn, use a phase-specific allocation recording. For retention, compare surviving objects and retaining paths after equivalent cycles. A full heap snapshot stops the main thread and can double heap use. Choose a target that can tolerate that impact. [Node CLI profiling](https://nodejs.org/docs/latest-v24.x/api/cli.html) · [Heap snapshots](https://nodejs.org/en/learn/diagnostics/memory/using-heap-snapshot)

## CPU, memory and loop timing over time

```bash
OCTOCODE_PROFILE_DIR="$out" OCTOCODE_PROFILE_INTERVAL_MS=1000 \
  node --import "$skill/scripts/node-metrics.mjs" app.mjs
# Set metrics to the emitted .metrics.jsonl file for the selected PID.
node "$skill/scripts/metrics-report.mjs" "$metrics" "$out/timeline.html"
```

The [metrics preload](../scripts/node-metrics.mjs) samples the main thread of a launched program, preserves application stdout and writes a mode-0600 JSONL file. The interval above is illustrative. Choose it for the event duration. The timer allows normal exit. Check the last record because an interrupted run can lack its exit measurement. The [offline HTML report](../scripts/metrics-report.mjs) accepts one capture and a new output path. It plots elapsed seconds and includes the underlying interval values.

| Measurement | Interpretation |
|---|---|
| CPU | Difference consecutive user + system CPU milliseconds and divide by actual elapsed milliseconds. Multiply by 100 for percent of one core. |
| Memory | RSS covers the process. Heap and external bytes cover the main isolate. Array buffer bytes are included in external memory. |
| Loop and GC | ELU measures loop activity. Delay p50/p99/max accumulate from preload. Difference GC count/duration for an interval. A blocked loop delays this sampler, so use an external probe to resolve work inside the gap. |
| Resource counters | Difference lifetime counters where meaningful. maxRSS is a high-water mark in KiB. fsRead/fsWrite count block I/O operations, with platform-dependent availability. |

The HTML connects observations with interpolated lines. Interval averages hide short peaks, and a cumulative maximum cannot date a later stall. Compare with an equivalent uninstrumented run to assess sampling/write overhead. [Node performance hooks](https://nodejs.org/docs/latest-v24.x/api/perf_hooks.html) · [Node process metrics](https://nodejs.org/docs/latest-v24.x/api/process.html)
