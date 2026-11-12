---
name: octocode-debugging
description: "Use when investigating a program’s runtime behavior, debugging a failure, profiling a workload, or interpreting a traceback, dump, profile or trace."
---

# Octocode Debugging

Let the user’s question determine what evidence would settle it. An existing traceback might already explain a failure. A live stall can disappear on restart. Launching under a debugger or profiler can reveal startup behavior. On a live target, account for the effect of pausing it and resume it if you stopped it.

For an incorrect result, ask **where the state first diverged**. A conditional breakpoint can isolate the affected request. A data watchpoint can identify the write that changed its value. For a performance question, ask **what the measurement counts**. CPU samples locate running code, wall time includes waits, and allocation profiles can describe churn or retention. Choose the scope and tool that fit the question.

## Optional references

| What you’re investigating | Where to look |
|---|---|
| A wrong value, exception, hang or crash | [Debugger techniques, dumps and replay](references/debugger-techniques.md) · [Controlled experiments and reproductions](references/experiments.md) |
| CPU, memory, I/O, scheduling or latency | [Profiling and runtime gotchas](references/profiling.md) · [Browser and GPU timelines](references/gpu.md) · [Full-stack requests](references/experiments.md#follow-a-full-stack-request) |
| Runtime-specific commands | [Native debugging](references/native.md) · [Rust profiling](references/rust-profiling.md) · [Go](references/go.md) · [Node debugging](references/node.md) and [profiling](references/node-profiling.md) · [Python debugging](references/python.md) and [profiling](references/python-profiling.md) |
| Sharing a finding | [Output options](output.md) · [Optional HTML renderer](scripts/render-report.mjs) |

Answer in the form the user needs. Tie a diagnosis to the observed state or cost and say what remains uncertain when it affects the conclusion.
