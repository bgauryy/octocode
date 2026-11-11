# Output

A short report in task terms.

- **Cause:** the file and line, the value or state observed there, what it should have been, and the command that showed it.
- **Fix:** what changed, and the reproduction and tests rerun after it with their result.
- **Profiling:** workload/input size, command/action, runtime/build, headless/visible mode, capture duration/sample count and artifact paths. Give CPU self/inclusive hotspots, memory trend and allocation/retention distinction, process/tree ownership, loop/GC timing and I/O/GPU evidence where measured. State units, profiler overhead and uncovered dimensions. If code changed, compare repeated identical uninstrumented runs; otherwise name the next experiment, not an invented speedup.
- **Not established:** what the runs could not show (for example a race that did not reproduce, or a build without debug information).

Save profiles, debugger logs and scratch programs to `<workspace>/.octocode/octocode-debugging/`, and remove temporary `debugger;` statements, `breakpoint()` calls and debug prints from the source.

## Visual evidence

Lead with the established finding and confidence, then use a compact table:

| Phase / request | Wall ms | CPU ms / % basis | RSS / live heap | Loop / queue / I/O | Evidence |
|---|---|---|---|---|---|
| Measured window | Value and count | Interval and process/thread scope | Units and lifecycle point | Measured or unknown | Artifact + selected range |

For Node JSONL, generate the offline [metrics report](scripts/metrics-report.mjs). For CPU, link the raw profile plus top self/inclusive rows and flamegraph; for event ordering, use a timeline. For memory, plot equivalent work cycles and show allocation/retainer evidence. For full stack, align request/span lanes and disclose clock uncertainty. Use one observed request path rather than a decorative architecture graph. Never invent a chart, merge incompatible capture clocks, or label cumulative quantiles as interval values.

Record each experiment's hypothesis, rival, prediction, observation and decision. Separate measured facts, causal inference and unknowns. Give repeated baseline/candidate results with sample count, spread and workload parity when claiming an improvement. End with one discriminating next experiment if the cause remains unresolved.
