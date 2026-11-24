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

| Need | Option |
|---|---|
| One phase of a longer run (start, a streamed reply, idle) | `--from <s> --to <s>`, seconds from the profile's start; read phase boundaries from the driver's timestamps |
| Who reaches a hot function | `--callers <name>`: the caller chains by the cost under them |
| Cost inside libraries or runtime internals (streams, promise machinery) | `--owner <path of your code>`: each cost lands on the nearest frame of yours, the call site you can change |

Each isolate writes its own `.cpuprofile` (the main thread and one per worker). V8's garbage-collector and compiler threads appear in none of them, yet can take a quarter of the process CPU under churn: compare the profiles' busy time with per-thread OS CPU (`ps -M -p "$pid"` on macOS, `pidstat -t -p "$pid"` on Linux) over the same window.

## Allocation churn

`node --heap-prof` keeps only what is still live when the profile is written, so short-lived garbage (buffers, promises, strings) is missing; a run that allocates gigabytes can show megabytes. To count churn, sample every allocation, collected ones included:

```bash
NODE_ALLOC_DIR="$out" node --import "$skill/scripts/node-alloc.mjs" app.mjs
node "$skill/scripts/profile-top.mjs" "$out" --owner "$PWD/src" --top 15 --ops "$operations"
```

The [allocation preload](../scripts/node-alloc.mjs) covers the main thread and writes `alloc-<pid>.heapprofile` at exit; `NODE_ALLOC_GATE=1` starts it when `"$out/start"` exists and writes it when `"$out/stop"` exists, so a driver can bracket one phase (`node "$skill/scripts/node-alloc.mjs" --help` lists its variables). `--ops` takes the number of operations the window covered and adds `self bytes/op` and `total bytes/op` columns: bytes per operation is deterministic where wall time and RSS are not. For retention, compare surviving objects and retaining paths after equivalent cycles. A full heap snapshot stops the main thread and can double heap use. Choose a target that can tolerate that impact. [Node CLI profiling](https://nodejs.org/docs/latest-v24.x/api/cli.html) · [Heap snapshots](https://nodejs.org/en/learn/diagnostics/memory/using-heap-snapshot)

## Blocking calls, startup and bundles

| Question | Measure |
|---|---|
| Which synchronous file or process calls run on the event loop | `node --trace-sync-io app.mjs 2>>"$out/sync.log"` prints a stack for each one after the first tick. Append (`2>>`) so the phase markers a driver appends to the same file (`echo "phase: <name>" >>"$out/sync.log"`) survive, then run `node "$skill/scripts/profile-top.mjs" "$out/sync.log" --sync-io --owner "$PWD/src"`: each row counts the synchronous operations of one phase, runtime API (such as `readFileSync`, whose open, stat, read and close each count) and first frame of your code |
| Is the compile cache used | `NODE_DEBUG_NATIVE=COMPILE_CACHE` logs whether each module's cache was accepted, rejected or written. `module.enableCompileCache()` covers the process that calls it; a launched child or worker needs its own call or `NODE_COMPILE_CACHE` |
| Why a worker or small entry is large | List the modules the bundler put in that entry (its metafile or module ids) and find the import chain to the heaviest package. Usually one import of a hub module or package root where a leaf module would do. Import graphs that do not mark type-only imports overstate what ships |
| A large string held from startup | A module-level `const` with a long literal (an embedded script, licenses) is created when the module loads; a function that returns it defers that to the first call |

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
