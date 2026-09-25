# octocode Engine: LSP defect history (protocol and tool level)

Load before copying a pattern from engine LSP code, when a regression looks familiar, or when reviewing an LSP diff. Why: every row is a bug octocode shipped. The guard and test named here are why the current code looks the way it does; removing either reopens the bug. Items are anchored by symbol, not line, because modules move.

E = `packages/octocode-native/crates/engine/src/lsp/`, R = `packages/octocode-native/crates/runtime/src/tools/lsp_search/`.

## Guards and regression tests
| # | Was | Guard now | Regression test |
|---|---|---|---|
| 1 | Rust without `rustContext` ran build.rs, proc-macros, and `cargo check` | `config::rust_analyzer_headless_options` in routes and `NativeLspClient::new`; user keys win; unexpanded-macro diagnostics carry a warning | `builtin_rust_route_runs_rust_analyzer_headless_by_default` |
| 2 | Mixed output bases | one choke point (`R/locations.rs`): lines and UTF-16 columns count from 1, diagnostics included; only the `position` input counts from 0 | `diagnostics_use_the_one_based_public_coordinates` |
| 3 | Call walk: DFS, flat, JSON-key dedupe, no node cap | `R/walk.rs`: BFS by level, `(canonical path, selectionRange)` key, merged edges with `level` + `via`, caps (values in `octocode-engine-map.md`), `next.continueWalk` | `hierarchy_walk_is_breadth_first_and_labels_every_edge_with_level_and_via` and the other `hierarchy_*` tests |
| 4 | `workspace/configuration` sent the whole blob per item | `E/transport/server_requests.rs`: per `section`, dotted paths, own-section root, else `null` | `server_requests::tests` |
| 5 | No cancel on drop; a drop mid-write left a partial frame | one writer task that takes whole frames only; a dropped request frees its entry and queues `$/cancelRequest` | `dropped_mid_write_cannot_corrupt_the_stream` |
| 6 | A timeout left other waiters for 30 s | any failure fails all pending requests | `timeout_fails_all_pending_immediately` |
| 7 | Inline reply writes in the read loop | replies go through the writer queue with a deadline | `server_request_reply_does_not_block_reads_when_server_stops_reading` |
| 8 | stderr unbounded and stopped on bad UTF-8 | `read_capped_line`, lossy decoding, keeps draining | `stderr_drain_survives_invalid_utf8` |
| 9 | No `staleRequestSupport`/serverStatus; declared `didSave` | declared; quiescent feeds readiness; `didSave` removed | `initialize_declares_only_what_the_client_handles` |
| 10 | Unparseable bodies not counted; garbage length desynced | counted; a garbage length is fatal | `garbage_content_length_value_is_fatal` |
| 11 | No restart backoff | per-key backoff 250 ms → 30 s | `repeated_start_failures_back_off_exponentially_and_reset_on_success` |
| 12 | Pool key didn't follow symlinks | `normalize_workspace_root` canonicalizes | `canonical_key_follows_workspace_symlinks` |
| 13 | Lone `\r` not a line break | `resolver::LineIndex` | `line_index_breaks_on_crlf_lf_and_lone_cr` |
| 14 | Python default pylsp lacks calls and symbols | basedpyright → pyright (from `PATH` only) → pylsp | `python_route_prefers_basedpyright_then_pyright_then_pylsp` |
| 15 | Node cap dropped parents silently | each dropped parent gets its own `next.continueWalk…N` plus `payload.unexpandedParents` | `hierarchy_node_cap_truncates_with_an_executable_continuation` |
| 16 | Recovered aliases looked like direct references | rows carry `source:"recoveredAlias"` | `recovered_alias_references_are_labeled_in_output` |

## Older guards (no single named test)
| Incident | Guard |
|---|---|
| Oversized frame → reader returned → zombie reused → 30 s hangs | fatal frame error fails the connection and all pending requests (`E/transport/codec.rs`, `connection.rs`) |
| Unbounded header `read_line` → OOM | `take()`-bounded header line and block caps (`codec.rs`) |
| Parent-only kill orphaned `proc-macro-srv` and cargo | process group + `kill(-pgid)`, also on clean exit (`E/client.rs`) |
| `maxMemoryMb:0` → no Windows tree kill | Job Object tree kill decoupled from the cap (`E/spawn_limits.rs`) |
| `RLIMIT_AS` on macOS → EINVAL | platform gate |
| Dropped acquire stranded a start | cancel-safe start (`E/pool.rs` `run_cancellation_safe_start`) |
| TS cold references incomplete; TS diagnostics empty | `open_document_and_wait`; declare `publishDiagnostics`, accept unversioned |
| Unknown server request answered with `null` | `-32601` |

Rust-level guards are in `references/octocode-rust-defects.md`.
