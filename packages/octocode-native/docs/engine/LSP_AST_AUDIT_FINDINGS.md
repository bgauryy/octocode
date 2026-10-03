# LSP and AST audit — open findings

Audited 2026-09; re-verified against source 2026-09-22. The original pass scored
the LSP and AST modules with `clasify`/Jev; that scoring is not reproduced here —
only the conclusions that survived reading the code.

Re-verification retired two of the four original priorities: both were artifacts
of scoring test code and an error type without reading them (see *Retired*).

## P1 — LSP server auto-restart (open)

`client.rs::stop()` performs a spec-compliant teardown (`shutdown` → `exit` →
wait → hard-kill) and `pool.rs` RAII cleanup fires reliably. Missing: when a
language server crashes mid-session it stays dead — the next `lspSearch` fails
with no re-spawn path.

Fix in `crates/engine/src/lsp/pool.rs` (or `client.rs`):
- On unexpected process exit: increment a crash counter, apply capped backoff
  (500ms → 2s → 10s), re-spawn up to N times per window.
- After max retries: mark the server failed and return an `lspUnavailable` error
  to callers — never panic, never silently hang.
- Reset the crash counter after a successful post-restart request.

Effort: medium. The spawn path exists; the missing piece is wrapping the exit
watcher in a bounded retry loop.

## P4 — Evaluate petgraph for `ast_graph/` (open, optional)

`crates/runtime/src/tools/ast_graph/` uses a custom directed-graph
implementation. Its needs — cycle detection, topological sort, reachability to
depth N, unreachable-node detection — map onto petgraph's `is_cyclic_directed`,
`toposort`, `has_path_connecting`, and node filtering.

Optional spike: swap cycle detection in `ast_graph/algorithms.rs` for
`petgraph::algo::is_cyclic_directed`, confirm identical output on real repos,
then decide whether to move toposort/reachability too. Keep `ast_graph/graph.rs`
as the public API so callers never see petgraph. Low risk; the payoff is less
custom algorithm code to maintain. Not urgent — the custom impl works.

## Confirmed good — do not regress

- `crates/engine/src/lsp/pool.rs`: Drop-based RAII cleanup fires reliably.
- `crates/engine/src/lsp/client.rs`: shutdown sequence is LSP-spec-compliant.
- LSP spawn limits (`lsp/spawn_limits.rs`): RAII `MemoryCapGuard` + OS Job Object
  give kill-on-close process-tree reaping.
- `crates/runtime/src/tools/ast_rewrite/mod.rs`: `RewriteError` is a structured
  type (`code`/`message`/`details`/`terminal`/`next`), and a journal-backed
  rollback test (`interrupted_multi_file_transaction_is_rolled_back_from_journal`)
  covers mid-rewrite crash safety.

## Retired on re-verification (2026-09-22)

- **Harden the `ast_rewrite/mod.rs` tail.** The flagged range (lines 1401–1692)
  is `#[cfg(test)]` code — tests start at line 1321. `unwrap`/`expect`/`panic`
  are lint-denied (`clippy -D warnings`) on all non-test paths, and the
  mid-rewrite rollback test already exists. No action.
- **Add structured AST error types.** `RewriteError` is already structured and
  richer than the `ClassificationError` it was told to copy. No action.

## Not yet audited — follow-up candidates

- `crates/engine/src/lsp/resolver.rs` — server-binary resolution error paths.
- `crates/engine/src/lsp/validation.rs` — LSP param validation completeness.
- `crates/runtime/src/tools/ast_rewrite/journal.rs` — journal durability on partial write.
- `crates/runtime/src/tools/ast_rewrite/lock.rs` — concurrent-rewrite lock contention.
- `crates/runtime/src/tools/ast_graph/analysis.rs` — analysis layer over the graph.

## Tracking

| Rec | Status |
|---|---|
| P1: LSP auto-restart | open |
| P4: petgraph spike | open, optional |
