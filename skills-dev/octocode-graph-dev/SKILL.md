---
name: octocode-graph-dev
description: "Load this skill first when changing or benchmarking Octocode's native graph engine (extraction, linking, snapshots, detectors): it gives the engine map and baseline battery. Queries and viewer: octocode-graph."
compatibility: "Node 20+, ripgrep, the pinned clones in packages/octocode-benchmark/local/repos, and a built octocode launcher or native binary."
---

# Octocode graph dev

The same native engine backs the `graphBuild` and `graphQuery` tools over MCP and the CLI (`octocode-graph graphBuild '<json>'`, `octocode-graph graphQuery '<json>'`).

## Change the engine

1. Capture a [language battery baseline](references/battery.md#run) before changing linking, recall or timing. Keep the complete before receipts.
2. Find the owner and its focused tests in the [engine map](references/engine-map.md). Graph schemas, descriptions and limits text belong to core via the [contract pipeline](../octocode-dev/docs/DEVELOPMENT.md#contract-pipeline).
3. Write a failing case first. For per-language linking, put it in `ast_graph/link_tests/<language>.rs` and derive the expected binding from that language’s rules. Keep an unsettled site unresolved with a named reason, or document what justifies a `medium` or `low` link. A matching name alone cannot establish a target.
4. Fix the owner. Bump the [version](references/engine-map.md#versions) for the cached data the change invalidates. Run focused `cargo test` modules in `packages/octocode-native` and the [evaluation commands](../../packages/octocode-native/docs/engine/CODE_GRAPH.md#evaluation). Use the [repo task runner](../octocode-dev/SKILL.md) for `build:dev`, then check `graphBuild` and `graphQuery` on CLI and MCP per [tool checks](../../packages/octocode-benchmark/CHECKS.md#graphbuild-and-graphquery).
5. Repeat the battery against the rebuilt binary. Keep the after receipts, compare the two runs and investigate failing rows before reporting. Update the evaluation table, per-language limits and any settled or deferred decisions in [CODE_GRAPH.md](../../packages/octocode-native/docs/engine/CODE_GRAPH.md).

## Judge a result

Inspect each [miss or extra](references/battery.md#reading-a-row) against the source. The oracle matches text. Classify each new extra by reading the source: a real caller the oracle missed, or a wrong link; precision must hold for every row on wrong links. Accept a recall drop only for an explained removal of a name-guess link. A precision or recall change on any row, or a cache version bump, goes to the owner with before/after rows; status stays Needs work until accepted. `callLinkRate` and `coverage.linkage` measure extracted syntax sites. For a caller or absence claim, combine graph and `lspSearch` callers and verify the source.

Use the [battery guide](references/battery.md) for the rows, the per-row checklist, the before/after judgement and adding a grammar. Follow [output.md](output.md) for the BLUF, comparison table, unmeasured rows and saved reports. For accepted HTML, use [render-report.mjs](scripts/render-report.mjs).

## Related skills

Use `rust-best-practices` for open Rust design choices, `octocode-engineer` to review the graph modules’ boundaries, and `octocode-graph` to render a repository’s architecture.
