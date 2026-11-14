---
name: octocode-debugging
description: "Use when something fails or is slow at runtime: crashes, wrong results, hangs, leaks, memory growth, slowness, flaky tests, tracebacks, dumps or profiles. Load it before guessing a fix: it finds the first divergence or the measured cost with debuggers and profilers. Use octocode-research for source-only questions."
---

# Octocode Debugging

Inspect surviving evidence before restarting or reproducing: a live stall can disappear on restart. For wrong results, find **the first state divergence**; for performance, establish **what the measurement counts**. CPU samples show running code, wall time includes waits, and allocation churn differs from retention.

## Procedure

1. **Inspect** the traceback, dump, log or saved profile. If it settles the question, answer from that evidence.
2. **Observe** the stop or measurement that separates rival causes. A conditional breakpoint isolates an invocation; a data watchpoint finds the writer. Verify a live target's identity and the impact of pausing it; ask before pausing, attaching to or signaling a process the request did not name. Resume or detach anything you stopped. Revise the probe when it leaves the cause unresolved. For performance and memory, follow the [profiling loop](references/profiling.md#the-profiling-loop).
3. **Fix** the first divergence when requested. Add a test that fails before the fix and passes after (check the pre-fix state in a separate worktree or checkout at the base revision; never revert, stash or check out in a shared tree), and search for the same pattern elsewhere. Replay the failure and nearby cases with the same input, environment and measurement; compare before and after on equivalent windows.
4. **Clean up**: stop the processes you started, remove temporary breakpoints, `debugger;` statements, logs and unsupported experimental edits, and keep the evidence the answer cites.

In commands, `skill` is this skill's absolute folder, `out` an existing capture directory, `pid` a verified target and `seconds` the observation window. A capture window is an experiment parameter, not a timeout for normal application work. Check installed tools and their `--help` for supported events and permissions; do not install profilers or change host permissions implicitly. Preserve the raw recording and its diagnostics.

## Optional references

| What you’re investigating | Where to look |
|---|---|
| A wrong value, exception, hang or crash | [Debugger techniques, dumps and replay](references/debugger-techniques.md) · [Controlled experiments, reproductions and bisecting](references/experiments.md) |
| CPU, memory, I/O, scheduling or latency | [Profiling loop, live targets and runtime gotchas](references/profiling.md) · [Memory: GC, heap, references, malloc/calloc, struct layout](references/memory-profiling.md) · [Who is using the machine](references/host-usage.md) · [Browser and GPU timelines](references/gpu.md) · [Full-stack requests](references/experiments.md#follow-a-full-stack-request) |
| Runtime-specific commands | [Native debugging](references/native.md) · [Rust profiling](references/rust-profiling.md) · [Go](references/go.md) · [Node debugging](references/node.md) and [profiling](references/node-profiling.md) · [Python debugging](references/python.md) and [profiling](references/python-profiling.md) |
| A long log, test run or CI output | Save it under the workspace. Search known markers with `localSearch`; use `clasify` for a typed question when exact search cannot narrow the unread evidence. [Tool usage](references/octocode-usage.md) |
| The code behind a frame, log line or symbol | Octocode local tools: `localSearch` for the text, `localFetch` at the line, `lspSearch` callers or references for who reaches it. [Tool usage](references/octocode-usage.md) |
| Sharing a finding | [Output options](output.md) · [Optional HTML renderer](scripts/render-report.mjs) |

Done when the evidence answers the question or identifies a concrete blocker; a requested fix also needs a replay check. Tie the diagnosis to observed state or cost, report uncertainty and unrun checks, and use the form the user needs.
