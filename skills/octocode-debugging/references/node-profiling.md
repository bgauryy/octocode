Load when a Node program's CPU, blocking calls, startup or bundle size must be attributed. For allocation churn per operation, or CPU, memory and event-loop timing charted over a run, load [allocation churn and metrics over time](node-metrics.md).

# Node.js profiling

## Already running

Identify the worker handling the request. For an existing private inspector, [attach DevTools to that Node target](node-devtools.md#attach-with-chrome-devtools) and record the operation in Performance as described there. If the inspector is unavailable, native sampling from the PID can still show worker, addon or runtime activity. Report which isolates, workers and child processes the capture covers. [Chrome Node Performance](https://developer.chrome.com/docs/devtools/performance/nodejs)

## Launch

Use the production entry point and TypeScript loader, and a fresh `out` directory per run.

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

## Blocking calls, startup and bundles

| Question | Measure |
|---|---|
| Which synchronous file or process calls run on the event loop | `node --trace-sync-io app.mjs 2>>"$out/sync.log"` prints a stack for each one after the first tick. Append (`2>>`) so the phase markers a driver appends to the same file (`echo "phase: <name>" >>"$out/sync.log"`) survive, then run `node "$skill/scripts/profile-top.mjs" "$out/sync.log" --sync-io --owner "$PWD/src"`: each row counts the synchronous operations of one phase, runtime API (such as `readFileSync`, whose open, stat, read and close each count) and first frame of your code |
| Is the compile cache used | `NODE_DEBUG_NATIVE=COMPILE_CACHE` logs whether each module's cache was accepted, rejected or written. `module.enableCompileCache()` covers the process that calls it; a launched child or worker needs its own call or `NODE_COMPILE_CACHE` |
| Why a worker or small entry is large | List the modules the bundler put in that entry (its metafile or module ids) and find the import chain to the heaviest package. Usually one import of a hub module or package root where a leaf module would do. Import graphs that do not mark type-only imports overstate what ships |
| A large string held from startup | A module-level `const` with a long literal (an embedded script, licenses) is created when the module loads; a function that returns it defers that to the first call |
