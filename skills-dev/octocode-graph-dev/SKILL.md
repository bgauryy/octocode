---
name: octocode-graph-dev
description: "Load this skill first when changing or benchmarking Octocode's native graph engine (extraction, linking, snapshots, detectors): engine map, cache versions and the language battery."
---

# Octocode graph dev

Graph tools run from `node packages/octocode-graph/bin/octocode-graph.mjs graphBuild|graphQuery '<json>'` (`mcp`: the server), not the `octocode` CLI. `$DEV`: [AGENTS.md](../../AGENTS.md#work-locally).

## Change the engine

1. Before changing linking, recall or timing, run the [language battery](references/battery.md) on a `$DEV pin` of the current binary; keep its receipts. If `pin` answers `noBinary` or native sources changed since the build, `$DEV build:dev` before editing, so the baseline measures the unchanged engine.
2. Find the owning module, its tests and the cache version it may invalidate in the [engine map](references/engine-map.md).
3. Write the failing case first; per-language linking cases go in `ast_graph/link_tests/<language>.rs`, expected target from the language's binding rules, never a matching name; leave an unsettled site unresolved with a named reason, or document why a link is `medium` or `low`.
4. Fix the owner, bump the [cache version](references/engine-map.md#cache-versions) the change invalidates, and test with `$DEV test:dev --crate tool-graph [--crate engine] [-- <module filter>]`. Run CODE_GRAPH.md's evaluation suite as `--crate engine -- graph::` plus `--crate tool-graph -- ast_graph`, not its bare `cargo test` lines.
5. After `build:dev` and a new pin, run the graph calls of [tool checks](../../packages/octocode-benchmark/CHECKS.md#graphbuild-and-graphquery) on CLI and MCP, then rerun the battery on that pin and [judge](references/battery-judge.md) every row against the baseline.

Stop when every battery row is judged and [CODE_GRAPH.md](../../packages/octocode-native/docs/engine/CODE_GRAPH.md#evaluation)'s evaluation, per-language limits and open decisions match it. Return the result, then per row before → after (recall, precision, link rate, build ms) explained from source, the cache version bumped or why none, tests run, and recall or precision changes for the owner to accept ([HTML report](references/battery.md#run) on request).
