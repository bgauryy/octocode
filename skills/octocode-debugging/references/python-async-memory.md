Load when a Python async handler is slow while the event loop looks idle, or Python memory grows across work cycles.

# Python async latency and allocation growth

`py`, `out` and `app.py` mean what they do in [Python profiling](python-profiling.md): the workload's Python executable, an existing capture directory and the real entry point.

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
