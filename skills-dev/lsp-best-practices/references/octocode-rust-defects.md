# octocode Engine: Rust implementation defect history

Load before copying Rust patterns from the engine LSP code (locks, tasks, errors, file reads, pooling), or when reviewing a Rust diff in the LSP engine or `lspSearch` runtime. Why: each guard encodes a Rust/tokio pitfall that looks correct in review, so don't "simplify" it away. Anchored by symbol; re-check before citing. The garbage-`Content-Length` guard is row 10 of `octocode-known-defects.md`.

E = `crates/engine/src/lsp/`, R = `crates/runtime/src/tools/lsp_search/`.

| # | Was | Guard now | Test |
|---|---|---|---|
| R1 | **Server-supplied URIs were read before policy and unbounded** (`/dev/zero`, FIFO) | `read_bounded_regular_file` (`is_file` before and after open, `O_NONBLOCK`, `take(MAX+1)`); `SnippetReadPolicy::with_authorizer` checks before any read; the runtime wires `PathPolicy` and drops `SNIPPET_CONTENT_WITHHELD` rows | `snippet_reads_reject_fifos_and_devices_without_blocking`, `unauthorized_snippet_paths_are_never_touched` |
| R2 | LRU eviction stopped busy clients | `evict_overflow` skips busy entries (temporary overflow allowed) | `lru_eviction_never_stops_a_busy_client_and_evicts_it_once_idle` |
| R3 | "busy" counted requests only | `NativeLspClient::lease()` → `LspLease`; syncs and waits hold leases; the runtime leases the whole `execute` | `lease_marks_the_client_busy_until_every_holder_releases` |
| R4 | `didChange v2` could race ahead of `didOpen v1` | per-client async lock across version reserve + notify | `concurrent_syncs_of_one_document_reach_the_server_in_version_order` |
| R5 | RPC code rendered into a string and parsed back out | `Error` is `Clone`, small (boxed detail), with `kind()`/`rpc_code()`; `ErrorCode::Other(i64)`; runtime `LspFailure::from_engine` maps kinds to `lsp.timeout` / `lsp.serverCrashed` / `lsp.capabilityUnavailable` / `lsp.requestFailed` | `rpc_errors_are_typed_through_the_connection`, `error::tests` |
| R6 | Runtime cancellation was checked only at entry | `cancellable()` around acquire, readiness, and every request; checks between hops and walk levels | runtime cancellation test (never-resolving future) |
| R7 | Sync `rust-analyzer --version` blocked the Node thread | bounded probe (3 s) in its own process group, killed as a group; async napi `isCommandAvailableAsync` | `command_probe_timeout_kills_the_whole_process_group` |
| R8 | Whole-file re-read per location; anchor file read 3× | `R/source.rs` `SourceCache` (one read and one line index per file per request); the anchor resolves from the exact `didOpen` text | source cache tests |
| R9 | Alias recovery could read 100 × 1 MB files | caps files **read** (32) and verified imports (32) | recovery tests |
| R11 | Result cloned; double serialization; two writes | moved out of the message; serialized once into one buffer | transport tests |
| R12 | Detached read loop; scattered `failed` stores | owned tasks aborted on drop; a guard created before spawn fails the connection on every exit | `dropping_the_connection_aborts_both_tasks` |
| R13 | Partial-result buffer leaked on drop | guard finishes the token | `dropped_partial_request_releases_its_token_buffer` |
| R14 | `exit` skipped after a slow `shutdown` | `notify_best_effort("exit")` | `exit_is_still_sent_after_a_timed_out_shutdown` |
| R15 | `Handle::try_current()` in `Drop` skipped cleanup off-runtime | the runtime handle is captured at guard creation | `stop_on_drop_outside_a_runtime_thread_still_stops_the_client` |
| R16 | One timer task per acquire; pool swap race | one abortable timer per entry; swap first, then clear; poisoned lock recovers | `one_idle_timer_per_entry_across_many_acquires` |
| R17 | `u64 as u32` on server positions | `u32::try_from` → InvalidArg | `parse_position_rejects_values_beyond_u32_instead_of_wrapping` |
| R18 | God files | `E/transport/{codec,connection,progress,partial,push_diagnostics,server_requests}.rs`; `R/{anchor,source,ops,recovery,walk,locations,failure,receipt,render}.rs` | n/a |
| R19 | Gap between pool acquire and lease let eviction stop a just-acquired client | `LspClientPool::acquire_leased` takes the lease under the pool lock | `acquire_leased_takes_the_lease_under_the_pool_lock` |
| R20 | A crash mid-request left waiters until timeout | crash fails pending as `lsp.serverCrashed`; no leaked server or grandchild PIDs | `R/process_tests.rs`: `server_crash_mid_request_fails_pending_fast_as_server_crashed`, `acquire_stop_loop_leaves_no_child_processes` |

## Still open
- The TS `node_modules` lookup still trusts the checkout (the existing trust level, documented in `LSP_SERVER_LIFECYCLE.md`); pyright is `PATH`-only.
- `RLIMIT_AS` 4 GiB is skipped on macOS; the Linux-only `node_server_starts_under_the_default_address_space_cap` proves Node starts under it. JVM servers (jdtls) reserve about ¼ of RAM for the heap by default and likely fail above 16 GB of RAM → consider an RSS guard or a higher JVM default.

Fix new issues with `references/rust-implementation.md` and prove them with `references/testing.md`.
