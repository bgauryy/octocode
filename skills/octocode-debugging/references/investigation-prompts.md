Load when starting an investigation or when attempts stop producing new evidence. Replace placeholders with observed project details; these prompts guide action, not just a written plan.

# Investigation prompts

## Debug a failure

> Investigate <symptom> using <command/request/input>. Read the complete error and relevant code, inspect recent changes and compare a working case. Preserve the failing input and expected versus actual result. State a leading hypothesis, its strongest rival and an observation that distinguishes them. Run the smallest useful probe: structured boundary logs, a conditional breakpoint, locals or thread stacks. Trace the first incorrect value back to its producer. Keep a failing regression check before changing behavior, fix the established cause, then rerun the reproduction and affected tests. Remove temporary probes. Report evidence, remaining uncertainty and exact verification results.

## Profile Node or Rust

> Profile <workload> on <runtime/build> for <CPU/memory/latency symptom>. Record the unchanged baseline, workload size, concurrency and cold/warm phase. Choose the capture from the symptom: CPU stacks for busy work; spans and thread stacks for waits; allocation and retention evidence for growth. Verify the target PID and attach permissions. Use the skill's Node or Rust capture instructions and scripts; check installed flags. Record one controlled workload window and preserve raw artifacts. Inspect self cost and inclusive callers, process/thread scope and profiler overhead. Test a rival explanation before optimizing. Compare repeated identical uninstrumented baseline/candidate runs and their spread. Report measured limits rather than inventing a bottleneck or speedup.

## Add diagnostic logs or chase a spike

> Instrument <suspect boundary> with structured events containing request/task ID, phase, monotonic duration, input size and expected/actual state. Use the existing logger or tracing subscriber. Avoid sensitive values, expensive formatting and mutations in probes. Capture before, during and after <trigger>; preserve seed, order and concurrency for intermittent failures. Choose sampling resolution appropriate to the event and disclose missed windows or cumulative metrics. Correlate logs with same-window stacks or spans. Remove temporary instrumentation and repeat with probes disabled to check perturbation.

## Correlate a full-stack request

> Follow one reproducible <interaction/request> across browser, Node, Rust and storage. Propagate its request/trace ID; record clock offsets or uncertainty. Capture browser Performance/network evidence, a dedicated Node profile and native Rust stacks or instrumented async spans as needed. Separate queue wait, execution and downstream time along the critical path. Do not sum overlapping spans or treat similar chart shapes as causality. Return aligned evidence lanes and one experiment that tests the suspected bottleneck.

## Recover when stuck

> List observations separately from assumptions and attempted changes. What did the last experiment rule out? Did input, runtime, optimization, concurrency or instrumentation change? Revert only your own unsupported experimental edits. Compare the failing case with the nearest working case and inspect the first divergent boundary. Choose one probe that could disprove the current explanation. If evidence remains unavailable, state the missing artifact or permission and the next action; do not declare a cause.

## Completion check

| Claim | Required evidence |
|---|---|
| Cause established | Reproduction plus a probe separating the cause from a plausible rival |
| Bug fixed | Regression check fails before the fix and passes after; original reproduction passes |
| Faster | Repeated comparable uninstrumented runs, sample count and spread |
| Memory leak | Growth after equivalent cleanup cycles plus retention/ownership evidence |
| Thread or async stall | Stacks/spans showing the wait and evidence about the expected progress |
| Unresolved | Known observations, unavailable scope and one discriminating next probe |
