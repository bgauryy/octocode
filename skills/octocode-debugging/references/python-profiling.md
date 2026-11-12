# Python profiling

Use the environment that runs the workload. In the examples, `py` is its Python executable, `out` is an existing capture directory, and `app.py` stands for the real entry point. A saved capture can go straight to interpretation.

## Function costs and call counts

For a launch, the standard library's cProfile records call events in the profiled thread. Use `-m package.module` in place of the script when that is how the application runs.

```bash
"$py" -m cProfile -o "$out/calls.pstats" app.py
"$py" - "$out/calls.pstats" <<'PY'
import pstats, sys
stats = pstats.Stats(sys.argv[1])
stats.sort_stats("tottime").print_stats(15)
stats.sort_stats("cumtime").print_stats(15)
PY
```

Self time (`tottime`) excludes callees. Inclusive time (`cumtime`) includes them. If an expensive helper has many callers, `stats.print_callers("helper_name")` breaks down their contributions. For a single operation in a larger program, `with cProfile.Profile() as profile:` around that operation and `profile.dump_stats(path)` give the same artifact. [cProfile and pstats](https://docs.python.org/3.14/library/profile.html)

**Gotcha.** cProfile's default timer includes elapsed time in blocking calls. A top row for `time.sleep` can be accurate even if CPU usage was negligible. Call instrumentation also costs more on code that makes many Python calls. Use an uninstrumented measurement when estimating real throughput.

## Sample a live interpreter or a launch

On CPython 3.15+ with remote sampling available, the built-in sampler can attach to a verified worker PID for an observation window. These examples are independent.

```bash
"$py" -m profiling.sampling attach -a --mode cpu -d "$seconds" --pstats -o "$out/cpu.pstats" "$pid"
"$py" -m profiling.sampling run --mode wall --gecko -o "$out/timeline.json" app.py
```

CPU mode samples threads on CPU. Wall mode includes waiting. The Gecko profile preserves time order for Firefox Profiler. Open sampled pstats with the producing interpreter’s pstats reader. Its counts represent sampled observations. Check thread coverage (`-a` includes all threads), and which process actually served the work. [CPython sampling modes and output](https://docs.python.org/3/library/profiling.sampling.html)

With a compatible py-spy installed, `py-spy record --pid "$pid" --duration "$seconds" -o "$out/stacks.svg"` captures a live process. For a finite launch, use `py-spy record -o "$out/stacks.svg" -- "$py" app.py`. The SVG groups sampled stacks by weight. The `--subprocesses` option can include Python workers. A `--gil` filter hides work in extensions that release the GIL. For a NumPy or ML workload with a hot native caller, supported native sampling or a device trace can resolve work below the Python frame. External sampling reads process memory. On some hosts that requires additional privileges even when launching a child. An in-process profiler can still work with the current permissions. [py-spy sampling and platform support](https://github.com/benfred/py-spy)

## Async latency

With Pyinstrument installed, `"$py" -m pyinstrument --timeline -r html -o "$out/run.html" app.py` records a wall-time timeline for a launch. If it mostly shows the event loop, sampling inside the handler can reveal which await held up that request. This example runs inside an async function with a distinct output path for the capture.

```python
from pyinstrument import Profiler

with Profiler(async_mode="enabled") as profile:
    result = await operation()
profile.write_html(output_path, timeline=True)
```

A long await locates the waiting caller. A downstream span can explain the wait. A thread-pool worker executes outside the profiled handler thread. [Pyinstrument async profiling](https://pyinstrument.readthedocs.io/en/latest/guide.html) · [Timeline options](https://pyinstrument.readthedocs.io/en/latest/reference.html)

## Allocation growth

For growth across work cycles, tracemalloc can compare live allocations at the same lifecycle point.

```python
import tracemalloc

tracemalloc.start()
before = tracemalloc.take_snapshot()
run_work_cycle()
after = tracemalloc.take_snapshot()
for change in after.compare_to(before, "lineno")[:10]:
    print(change)
```

Positive `size_diff` means more traced bytes survived at that allocation site. Objects allocated and freed between snapshots disappear from the difference. Untracked native buffers and allocator reserves can raise RSS while this difference stays flat. [tracemalloc snapshots](https://docs.python.org/3/library/tracemalloc.html)
