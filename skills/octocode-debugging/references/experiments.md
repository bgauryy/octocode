# Controlled experiments

When the same observation fits several explanations, look for a difference you can measure. A nearby working case can reveal which input, timing or concurrency condition matters.

| Observation | Rival explanations | Discriminating evidence |
|---|---|---|
| Tail latency rises with low process CPU | Downstream I/O, a contended local lock or a runnable worker starved of CPU | Same-request dependency span, lock owner/waiter or runnable-to-running scheduler delay |
| RSS climbs, live managed heap stays flat | Native retention or allocator reserves | Outstanding native allocations and mappings after equivalent cleanup cycles |
| Failure disappears at a breakpoint | Timing-sensitive race or changed input/order | Reuse the same input and concurrency. Collect low-overhead events around the first divergent handoff. |

## Narrow a reproducible failure

If a large input fails, define a precise test for *this* failure, then try removing input chunks or operations. Preserve dependencies needed for the input to remain valid. A parse error introduced by reduction is a different outcome. A small reproducer exposes the conditions under which the failure occurs. It still takes a runtime observation to locate the responsible code. [Delta debugging](https://www.debuggingbook.org/html/DeltaDebugger.html)

For a regression between a known good and bad revision, a repeatable test can drive `git bisect run` in a disposable checkout. Exit 0 for good, 1 for the reproduced failure and 125 for an untestable revision. Flaky outcomes can point bisect at the wrong commit. Recheck the boundary with the same input and environment. [Git bisect](https://git-scm.com/docs/git-bisect)

## Catch a spike

Record before, during and after the trigger. If the service already retains rolling telemetry, save the original window. Choose sampling resolution for the suspected burst, and record missed events, delayed samples and cumulative metrics. A one-second average cannot locate a ten-millisecond spike. Quantiles cannot be differenced across windows.

Use boundary events with a monotonic duration, operation ID, input size and phase. Include a reason for a retry or queue transition when relevant. Avoid sensitive payloads and per-item synchronous logging in a hot path. For intermittent failures, preserve the first failing run, the number of attempts and their outcomes. Replaying an external write needs an idempotent or disposable target. When an experiment fails, keep its evidence and remove unsupported experimental edits.

## Follow a full-stack request

A trace ID, parent/child span IDs or span links can connect a slow interaction to its server, queue or downstream dependency. Request IDs can join logs when spans are unavailable. Retries may share a trace ID. Look for the attempt that delayed completion.

A browser’s time to first byte includes transit and server work. To attribute that interval, use whichever boundary is visible, such as a connection-pool wait or a downstream span. Child spans can overlap. A hot stack shared by many concurrent requests establishes process activity. Per-request attribution requires a task/span association or an isolated replay.

If a timeline would clarify the finding, a shared time axis requires aligned clocks or measured offsets. Local durations and causal links remain usable across independent clocks. [OpenTelemetry traces](https://opentelemetry.io/docs/concepts/signals/traces/) · [W3C Trace Context](https://www.w3.org/TR/trace-context/) · [Perfetto clock synchronization](https://perfetto.dev/docs/concepts/clock-sync)
