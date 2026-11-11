# Octocode Debugging

Debug and profile running programs with measured evidence. Focus Node and Rust investigations on CPU, memory, processes/threads, event-loop/GC, async waits, I/O and spikes. Use a hypothesis → probe → observation loop before changing code.

| Need | Start here |
|---|---|
| Route the symptom | [SKILL.md](SKILL.md) |
| Plan logs, reproduction loops or full-stack experiments | [Controlled experiments](references/experiments.md) |
| Choose measurements and interpret their limits | [Profiling](references/profiling.md) |
| Attach dedicated Node Chrome DevTools | [Node](references/node.md) |
| Profile Rust CPU, allocations, threads and Tokio tasks | [Rust profiling](references/rust-profiling.md) |
| Capture Node metrics / draw offline timelines | [Metrics preload](scripts/node-metrics.mjs) / [HTML report](scripts/metrics-report.mjs) |
| Capture an authorized native PID on macOS/Linux | [Native capture](scripts/native-capture.sh) |
| Summarize V8 CPU or allocation profiles | [Profile summary](scripts/profile-top.mjs) |
| Deliver findings, visuals and uncertainty | [Output](output.md) |
| Check the research basis | [Primary sources](references/profiling-sources.md) |

Node scripts require Node 22 or later; native captures require the platform's installed profiler and attach permissions. Use batch commands for unattended debugging and interactive tools for visual inspection. Artifacts go under `<workspace>/.octocode/octocode-debugging/`. Keep raw evidence; label sample scope, units, timing and missing dimensions.
