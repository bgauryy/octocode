Load when naming a debugging or observability tool or technique: breakpoints, dumps, profiling, flame graphs, logs, metrics, traces, sanitizers or bisection.

# Concepts: debugging and observability

## Debugging and observability concepts

Debugging evidence is runtime evidence; it establishes the loaded artifact and scenario, not every configuration ([evidence](evidence.md#match-proof-to-the-claim)).

| Concept | Purpose | When to use |
|---|---|---|
| Breakpoint, conditional breakpoint | Pause execution at a line, or only when a condition holds | Inspect specific behavior; bugs under specific conditions |
| Watchpoint (data breakpoint) | Pause when a variable or memory changes | Unexpected state mutations |
| Logpoint | Log without stopping execution | Live execution |
| Step into, step over, step out | Enter a call, run it whole, or finish the current function | Follow control flow at the needed depth |
| Call stack, stack trace | Active calls; recorded call sequence | Find where an error originated; analyze crashes |
| Variable inspection, expression evaluation | Inspect runtime values; run expressions while paused | Find incorrect state; test assumptions |
| Core dump | Snapshot of a crashed process's memory | Native application crashes |
| CPU profiling (sampling, instrumentation) | Identify expensive execution paths; sampling is cheap, instrumentation is exact | Performance bottlenecks |
| Memory profiling, heap snapshot | Analyze allocations and retained objects | Memory leaks and retention |
| Flame graph | Visualize execution cost by stack | CPU and latency analysis |
| Logs, metrics, traces | The three observability signals: events, aggregates, request paths | Production debugging ([telemetry rules](systems-operations.md#observability-and-sre)) |
| Structured logging | Machine-readable events with fields | Searchable production logs |
| Distributed tracing | Follow one request across services | Microservices debugging |
| Correlation id | Connect logs belonging to one operation | Multi-component requests |
| Assertions | Validate expected invariants | Detect invalid states early |
| Sanitizers, race detector | Detect memory errors; detect unsafe concurrent access | Native code; multithreaded programs ([tools](memory-safety-tools.md#debugging-and-profiling-tools)) |
| Static analysis | Detect issues without executing code | CI and development |
| Symbolic debugging | Map machine addresses to source | Native binaries and crash dumps |
| Remote debugging | Debug another process or machine | Containers, servers, devices |
| Time-travel debugging | Replay execution history | Intermittent bugs |
| Bisection (`git bisect`) | Find the commit that introduced a bug | Regressions |
| Delta debugging | Minimize a failing input or configuration | Complex reproductions |
| Fault injection | Deliberately introduce failures | Resilience validation |
