---
name: octocode-debugging
description: "Load this skill first when a program crashes, hangs, leaks, runs slowly or returns a wrong or flaky result. Inspect runtime state, profile costs and replay the fix."
---

# Octocode Debugging

Inspect surviving evidence before restarting a live process. For wrong results, find the first divergent state. For performance, identify what the measurement counts. CPU samples show running code, wall time includes waits, and allocation churn differs from retention. If the program works but the assistant keeps choosing the same tool, `octocode-agentic` inspects the decision loop.

## Procedure

1. **Inspect** the traceback, dump, log or saved profile. If it settles the question, answer from that evidence.
2. **Observe** the stop or measurement that separates rival causes. A conditional breakpoint isolates an invocation; a data watchpoint finds the writer. Verify a live target's identity and the impact of pausing it; ask before pausing, attaching to or signaling a process the request did not identify (a named service counts once you find and verify its PID). Resume or detach anything you stopped. Revise the probe when it leaves the cause unresolved. For performance and memory, follow the [profiling loop](references/profiling.md#the-profiling-loop).
3. **Fix** the first divergence when requested. Add a test that fails before the fix and passes after (check the pre-fix state in a separate worktree or checkout at the base revision, or with the file from the base commit; never revert, stash or check out in a shared tree, and an index that tooling stages on its own is not a baseline), and search for the same pattern elsewhere. Replay the failure and nearby cases with the same input, environment and measurement; compare before and after on equivalent windows.
4. **Clean up**: stop the processes you started, remove temporary breakpoints, `debugger;` statements, logs and unsupported experimental edits, and keep the evidence the answer cites.

Before a capture, check installed tools and their `--help` for supported events and permissions; do not install profilers or change host permissions implicitly. Preserve the raw recording and its diagnostics.

## Optional references

In reference commands, `$skill` is this skill's absolute folder, `$out` an existing capture directory, `$pid` a verified target and `$seconds` the observation window (an experiment parameter, not an application timeout).

| What you’re investigating | Where to look |
|---|---|
| A wrong value, exception, hang or crash | [Debugger techniques, dumps, replay](references/debugger-techniques.md) · [Experiments, reproductions, bisecting](references/experiments.md) |
| CPU, I/O, waits or latency | [Profiling loop](references/profiling.md#the-profiling-loop), [comparable runs](references/profiling-runs.md) (`compare-runs.mjs` compares builds), [reading a measurement](references/profiling-readings.md), [platform, JVM and .NET captures](references/profiling-platforms.md) or [trace a full-stack request](references/experiments.md#follow-a-full-stack-request) |
| Growing memory or host load | [Allocation versus retention](references/memory-profiling.md) or [the process using the machine](references/host-usage.md) (`host-usage.mjs`), then [one process](references/host-process.md) |
| Browser frames or GPU stalls | [Browser and GPU timelines](references/gpu.md) |
| Native, Rust or Go target | [Native debugger](references/native.md) and [crashes or hangs](references/native-crash.md), Rust [profiler](references/rust-profiling.md), [memory](references/rust-memory.md) and [async tracing](references/rust-async.md), or [Go tools](references/go.md); `native-capture.sh` samples a verified PID |
| Node or Python target | Node [debugger](references/node.md) (`node-break.mjs`), [DevTools attach](references/node-devtools.md), [profiler](references/node-profiling.md) (`profile-top.mjs`) or [allocation and metrics timeline](references/node-metrics.md); Python [debugger](references/python.md), [profiler](references/python-profiling.md) or [async and allocation growth](references/python-async-memory.md). |
| A long log, test run or CI output | Save it under the workspace; search known markers with `localSearch`, and use `clasify` for a typed question exact search cannot narrow. |
| The code behind a frame, log line or symbol | `localSearch` for the text, `localFetch` at the line, `lspSearch` callers or references for who reaches it. |
| Sharing a finding | [Output options](output.md) · [HTML renderer](scripts/render-report.mjs) |

Done when the evidence answers the question or names a concrete blocker; a requested fix also needs a replay check. Tie the diagnosis to observed state or cost, and report uncertainty and unrun checks.
