Load when judging battery rows after a run: which recall, precision, timing or cache changes are regressions and which need the owner's acceptance.

# Judge the battery

- Precision must hold on every row. A recall drop, a workflow query that fails where it answered before, or `incremental.reused` turning false is a regression to explain from source; accept a recall drop only as an explained removal of a name-guess link.
- Read the source behind every miss and new extra. The oracle matches text, not bindings: a same-named function of another library or a binding built at runtime still counts as a miss (`missedSites` gives the graph's reason), prototypes and declarations the graph does not list still count as hits, and callers through `extern "C"` includes, re-exports or other names are real but invisible to it, so an extra can be correct.
- Any recall or precision change, or a cache version bump, needs the owner's acceptance with before → after rows.
- Timing claims: one row at a time on an idle machine; machine load moves build time more than most changes.
- Per-edge agreement with LSP: compare `witnesses` call edges with an octocode-graph LSP cross-check (`via: lsp | both | graph`).
- Viewer paging timing: same frozen tree, binary and warm snapshots. In the run's `events.jsonl`, count one `tool` start per id; a chunk's `paging` state confirms a native snapshot and its `done` state that the viewer read the pages. Compare streamed rows as multisets.

Keep the explained misses for each row current in [CODE_GRAPH.md](../../../packages/octocode-native/docs/engine/CODE_GRAPH.md#evaluation).
