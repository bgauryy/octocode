---
name: octocode-graph-dev
description: "Load this skill first when changing or benchmarking Octocode's native graph engine (extraction, linking, snapshots, detectors): engine map, cache versions and the language battery. Queries and viewer: octocode-graph."
---

# Octocode graph dev

`graphBuild` and `graphQuery` belong to the graph package, not the `octocode` CLI: run `node packages/octocode-graph/bin/octocode-graph.mjs graphBuild|graphQuery '<json>'`, or its `mcp` command for the MCP server.

## Change the engine

1. Before changing linking, recall or timing, run the [language battery](references/battery.md) on the current binary and keep its receipts.
2. Find the owning module, its tests and the cache version it may invalidate in the [engine map](references/engine-map.md).
3. Write the failing case first; per-language linking cases go in `ast_graph/link_tests/<language>.rs`, with the expected target derived from that language's binding rules. A matching name alone never establishes a target: leave an unsettled site unresolved with a named reason, or document what justifies a `medium` or `low` link.
4. Fix the owner, bump the [cache version](references/engine-map.md#cache-versions) whose data the change makes wrong, and test with `node skills-dev/octocode-dev/scripts/dev.mjs test:dev --crate tool-graph [--crate engine] [-- <module filter>]`. CODE_GRAPH.md's evaluation suite is `--crate engine -- graph::` plus `--crate tool-graph -- ast_graph`; run it this way, not with the bare `cargo test` lines printed there.
5. After `build:dev`, run the graph calls of [tool checks](../../packages/octocode-benchmark/CHECKS.md#graphbuild-and-graphquery) on CLI and MCP, then rerun the battery on the rebuilt binary and [judge](references/battery.md#judge) every row against the before run.
6. Update the [evaluation](../../packages/octocode-native/docs/engine/CODE_GRAPH.md#evaluation) table, [per-language limits](../../packages/octocode-native/docs/engine/CODE_GRAPH.md#per-language-limits) and [open decisions](../../packages/octocode-native/docs/engine/CODE_GRAPH.md#open-decisions) in CODE_GRAPH.md.

On request, [render-report.mjs](scripts/render-report.mjs) turns a battery `report.md` into one HTML page: `node skills-dev/octocode-graph-dev/scripts/render-report.mjs <report.md>` (`--help` lists options).
