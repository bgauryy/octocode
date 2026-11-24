---
name: octocode-debugging
description: "Load this skill first when a program crashes, hangs, leaks, runs slowly or returns a wrong or flaky result. Inspect runtime state, profile costs and replay the fix. Repeated AI tool calls belong to octocode-agentic."
---

# Octocode Debugging

Inspect surviving evidence before restarting a live process. For wrong results, find the first divergent state. For performance, identify what the measurement counts. CPU samples show running code, wall time includes waits, and allocation churn differs from retention. If the process is healthy and the assistant keeps choosing the same tool, use `octocode-agentic` to inspect the decision loop.

## Procedure

1. **Inspect** the traceback, dump, log or saved profile. If it settles the question, answer from that evidence.
2. **Observe** the stop or measurement that separates rival causes. A conditional breakpoint isolates an invocation; a data watchpoint finds the writer. Verify a live target's identity and the impact of pausing it; ask before pausing, attaching to or signaling a process the request did not identify (a named service counts once you find and verify its PID). Resume or detach anything you stopped. Revise the probe when it leaves the cause unresolved. For performance and memory, follow the [profiling loop](references/profiling.md#the-profiling-loop).
3. **Fix** the first divergence when requested. Add a test that fails before the fix and passes after (check the pre-fix state in a separate worktree or checkout at the base revision; never revert, stash or check out in a shared tree), and search for the same pattern elsewhere. Replay the failure and nearby cases with the same input, environment and measurement; compare before and after on equivalent windows.
4. **Clean up**: stop the processes you started, remove temporary breakpoints, `debugger;` statements, logs and unsupported experimental edits, and keep the evidence the answer cites.

In commands, `skill` is this skill's absolute folder, `out` an existing capture directory, `pid` a verified target and `seconds` the observation window. A capture window is an experiment parameter, not a timeout for normal application work. Check installed tools and their `--help` for supported events and permissions; do not install profilers or change host permissions implicitly. Preserve the raw recording and its diagnostics.

## Optional references

| What you’re investigating | Where to look |
|---|---|
| A wrong value, exception, hang or crash | [Debugger techniques, dumps and replay](references/debugger-techniques.md) · [Controlled experiments, reproductions and bisecting](references/experiments.md) |
| CPU, I/O, waits or latency | [Choose a measurement](references/profiling.md) or [trace a full-stack request](references/experiments.md#follow-a-full-stack-request). |
| Growing memory or host load | [Distinguish allocation from retention](references/memory-profiling.md) or [identify the process using the machine](references/host-usage.md). |
| Browser frames or GPU stalls | [Browser and GPU timelines](references/gpu.md) |
| Native, Rust or Go target | [Native debugger](references/native.md), [Rust profiler](references/rust-profiling.md) or [Go tools](references/go.md) |
| Node or Python target | Node [debugger](references/node.md) or [profiler](references/node-profiling.md). Python [debugger](references/python.md) or [profiler](references/python-profiling.md). |
| A long log, test run or CI output | Save it under the workspace. Search known markers with `localSearch`; use `clasify` for a typed question when exact search cannot narrow the unread evidence. |
| The code behind a frame, log line or symbol | Octocode local tools: `localSearch` for the text, `localFetch` at the line, `lspSearch` callers or references for who reaches it. |
| Sharing a finding | [Output options](output.md) · [Optional HTML renderer](scripts/render-report.mjs) |

Done when the evidence answers the question or identifies a concrete blocker; a requested fix also needs a replay check. Tie the diagnosis to observed state or cost, report uncertainty and unrun checks, and use the form the user needs.
