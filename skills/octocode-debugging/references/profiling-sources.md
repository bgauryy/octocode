Load to check why the Node/Rust profiling workflows choose separate measurement layers.

# Research basis

Checked 2026-11-11 against primary documentation. Recheck installed runtime/tool versions before using flags. Quotes below are deliberately short; linked documents own the details.

| Source | Verified excerpt | Consequence for this skill |
|---|---|---|
| [Chrome Node Performance](https://developer.chrome.com/docs/devtools/performance/nodejs) | “Use the Performance panel to profile the performance of Node.js and Deno applications.” | Dedicated Node target; select an interval, inspect Bottom-Up and Call Tree. |
| [Node heap snapshots](https://nodejs.org/en/learn/diagnostics/memory/using-heap-snapshot) | “You can also compare multiple snapshots to see differences over time.” | Compare equivalent lifecycles; acknowledge snapshot pauses and memory risk. |
| [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html) | “Profiles provide a way to alter the compiler settings, influencing things like optimizations and debugging symbols.” | Keep production optimization with symbols; do not benchmark debug builds as production. |
| [Tokio tracing](https://tokio.rs/tokio/topics/tracing) | “individual tasks are multiplexed on the same thread” | Use task/request spans alongside thread stacks. |
| [tracing span safety](https://docs.rs/tracing/latest/tracing/span/index.html) | “guard is held across an await point” | Avoid holding entered-span guards across await; instrument futures instead. |
| [Tokio Console](https://docs.rs/tokio-console/latest/tokio_console/) | “it must be instrumented to emit the data the console consumes” | Do not promise arbitrary PID attachment for task telemetry. |
| [tracing-chrome](https://docs.rs/tracing-chrome/latest/tracing_chrome/) | “outputs traces in Chrome’s trace viewer format” | Exported spans are visualizable; they are not native CPU samples. |
| [Massif](https://valgrind.org/docs/manual/ms-manual.html) | “It measures how much heap memory your program uses.” | Separate native live allocations from RSS and allocation churn. |
| [flamegraph](https://github.com/flamegraph-rs/flamegraph) | “if the executable is already running, you can provide the PID” | Native sampling can attach; permissions and platform still matter. |

Additional command/API references: [Node debugging](https://nodejs.org/en/learn/getting-started/debugging), [Node performance hooks](https://nodejs.org/docs/latest-v24.x/api/perf_hooks.html), [ChromeLayerBuilder](https://docs.rs/tracing-chrome/latest/tracing_chrome/struct.ChromeLayerBuilder.html), [Samply](https://github.com/mstange/samply).

## Decision and strongest objection

Recommendation (design judgment): keep symptom routing in SKILL.md; load Node attachment, Rust profiling and controlled experiments only when needed. Add an offline Node metrics timeline and a native PID capture wrapper, reusing existing capture tools rather than implementing another profiler.

CPU-only workflows remain useful for repeatable busy-core problems. They lose as the universal default because waiting, suspended futures and retained allocations require different evidence. The strongest objection is instrumentation overhead: counters, logs, traces and heap tools change timing. Start with an uninstrumented baseline, capture dimensions separately, and prefer the smallest probe that distinguishes rival explanations.

Research hypotheses: CPU-only misses waiting — supported by the distinction between event-loop utilization and CPU in Node performance hooks and task causality in Tokio tracing; one snapshot proves a leak — rejected by Node's comparison workflow; timeline windows help localize spikes — supported by Chrome's selected-interval workflow. These support measurement choices, not a claim that a bottleneck was found in this repository.

Limits: direct sequential source research; a separate read-only scenario check exercised routing and found the trace-argument omission, which was corrected. Nine short source excerpts checked manually against retrieved primary pages; no unverified excerpt retained. Commands remain subject to local tool availability. Linux perf and an instrumented Rust server need host-specific execution before claiming runtime validation there.
