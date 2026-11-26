Load when a Node program allocates too much per operation, or its CPU, memory, GC and event-loop timing must be charted over a run.

# Node.js allocation churn and metrics over time

## Allocation churn

`node --heap-prof` keeps only what is still live when the profile is written, so short-lived garbage (buffers, promises, strings) is missing; a run that allocates gigabytes can show megabytes. To count churn, sample every allocation, collected ones included:

```bash
NODE_ALLOC_DIR="$out" node --import "$skill/scripts/node-alloc.mjs" app.mjs
node "$skill/scripts/profile-top.mjs" "$out" --owner "$PWD/src" --top 15 --ops "$operations"
```

The [allocation preload](../scripts/node-alloc.mjs) covers the main thread and writes `alloc-<pid>.heapprofile` at exit; `NODE_ALLOC_GATE=1` starts it when `"$out/start"` exists and writes it when `"$out/stop"` exists, so a driver can bracket one phase (`node "$skill/scripts/node-alloc.mjs" --help` lists its variables). `--ops` takes the number of operations the window covered and adds `self bytes/op` and `total bytes/op` columns: bytes per operation is deterministic where wall time and RSS are not. For retention, compare surviving objects and retaining paths after equivalent cycles with [heap snapshots](memory-gc.md#heap-snapshots-and-retainers), on a target that can afford their pause and memory. [Node CLI profiling](https://nodejs.org/docs/latest-v24.x/api/cli.html) · [Heap snapshots](https://nodejs.org/en/learn/diagnostics/memory/using-heap-snapshot)

## CPU, memory and loop timing over time

```bash
NODE_METRICS_DIR="$out" NODE_METRICS_INTERVAL_MS=1000 \
  node --import "$skill/scripts/node-metrics.mjs" app.mjs
# Set metrics to the emitted .metrics.jsonl file for the selected PID.
node "$skill/scripts/metrics-report.mjs" "$metrics" --json                  # intervals as JSON
node "$skill/scripts/metrics-report.mjs" "$metrics" "$out/timeline.html"    # offline chart
```

The [metrics preload](../scripts/node-metrics.mjs) samples the main thread of a launched program, preserves application stdout and writes a mode-0600 JSONL file (`node "$skill/scripts/node-metrics.mjs" --help` lists its variables and records). The interval above is illustrative. Choose it for the event duration. The timer allows normal exit. The [metrics report](../scripts/metrics-report.mjs) reads one capture and differences consecutive records into intervals: `--json` prints them as `rows`, with `exitRecorded` false when the run was interrupted before its exit measurement; an output path writes the HTML chart of the same intervals.

| Row field | Interpretation |
|---|---|
| `cpuPercent` | User + system CPU over elapsed time in the interval; 100 is one core. |
| `rssMiB`, `heapMiB`, `externalMiB`, `maxRssMiB` | RSS covers the process; heap and external memory cover the main isolate, and array buffers are included in external memory. `maxRssMiB` is the process high-water mark so far. |
| `loopUtilization`, `loopDelayMaxMs`, `gcCount`, `gcMs` | The event loop's active share of the interval; the delay maximum accumulates from preload; collections and their duration in the interval. A blocked loop delays this sampler (`intervalMs` longer than the setting), so use an external probe to resolve work inside the gap. |
| `fsRead`, `fsWrite` | Block I/O operations in the interval, with platform-dependent availability. |

The HTML connects observations with interpolated lines. Interval averages hide short peaks, and a cumulative maximum cannot date a later stall. Compare with an equivalent uninstrumented run to assess sampling/write overhead. [Node performance hooks](https://nodejs.org/docs/latest-v24.x/api/perf_hooks.html) · [Node process metrics](https://nodejs.org/docs/latest-v24.x/api/process.html)
