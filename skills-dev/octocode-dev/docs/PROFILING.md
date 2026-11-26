Load when measuring a tool call's instructions, memory or heap.

# Profiling

## Profiling a tool flow

```bash
node scripts/build-slot.cjs -- cargo build --locked --profile profiling -p octocode-cli   # in packages/octocode-native
$DEV profile localSearch '{"path":"crates","matchString":["fn "]}' --cwd packages/octocode-native --phase repeat
samply load .octocode/octocode-dev/profile/<run>/profile.json.gz
```

`--phase` is `first` (a fresh MCP server's first call), `repeat`, `page:N` or `one` (a whole one-shot CLI process). The run prints min-of-3 retired instructions and peak footprint without a sampler, then samples a server looping that phase and writes `top.md` and `counters.json` under `.octocode/octocode-dev/profile/`. Trust instructions, not wall time. `--heap` adds what the server still holds afterwards (`malloc_history` on macOS, `heaptrack` on Linux).

A plain CLI call is a thin client of the shared warm `octocode serve` process (`crates/cli/src/cli/serve.rs`), so `time -l` on it measures nothing useful. `OCTOCODE_STORAGE_MODE=memory` runs in-process but refuses any answer that needs a continuation, so it cannot measure large answers. For a graph cursor, put the snapshot in the target workspace's graph store, use that workspace as `--cwd`, request `--phase page:2`, and check the counters' error list before comparing.
