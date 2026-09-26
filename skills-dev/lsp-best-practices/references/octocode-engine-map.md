# octocode Engine Map: where LSP lives, bounds, tests

Load when touching `octocode-native` LSP code. Use it to find the owning file, reuse an existing helper or bound, and pick the right tests. Why: the client already has a bound or guard for most problems, and a new parallel mechanism is drift. Anchored by symbol; `grep -n` the symbol before citing a line.

E = `packages/octocode-native/crates/engine/src/lsp/`, R = `packages/octocode-native/crates/runtime/src/tools/lsp_search/`.

## Ownership
| Concern | Owner |
|---|---|
| Frame read/write, size caps | `E/transport/codec.rs` (`MAX_CONTENT_LENGTH`, header caps) |
| Connection: writer task, pending map, cancel on drop, fail-all, `notify_best_effort` | `E/transport/connection.rs` (+ `connection_tests.rs`) |
| Readiness (`$/progress`, serverStatus quiescent) | `E/transport/progress.rs` (`ProgressTracker`) |
| Partial results / push diagnostics | `E/transport/partial.rs` / `push_diagnostics.rs` (each owns its caps) |
| Replies to server requests (`configuration` per section, …) | `E/transport/server_requests.rs` |
| Typed errors (`ErrorKind`, `RpcError`, `ErrorCode`) | `engine/src/error.rs` |
| Spawn, init params, stop/Drop, documents, leases, bounded snippet reads | `E/client.rs` (`LspLease`, `SnippetReadPolicy`, `read_bounded_regular_file`, `drain_stderr`) |
| Pool: key, dedupe, busy-aware LRU, idle timer, restart backoff | `E/pool.rs` (`evict_overflow`, `StartBackoff`, `readiness_timeout`) |
| Routes, headless defaults, probe | `E/config.rs` (`rust_analyzer_headless_options`, `resolve_pyright_family`, `probe_command_succeeds`) |
| Anchor → position, line index | `E/resolver.rs` (`LineIndex`, `resolve_position_in_file_content`) |
| Memory cap / Job Object; root markers | `E/spawn_limits.rs`; `E/workspace.rs` |
| Tool entry, lease, cancellation | `R/mod.rs` (`execute`, `cancellable`) |
| Anchor, source cache, ops | `R/anchor.rs`, `R/source.rs` (`SourceCache`), `R/ops.rs` |
| Definition chain, alias recovery | `R/recovery.rs` |
| Call/type hierarchy BFS | `R/walk.rs` |
| Public coordinates, pagination, errors, receipts | `R/locations.rs` (1-based choke point), `R/failure.rs` (`LspFailure::from_engine`), `R/receipt.rs` |
| Managed install; napi | `crates/cli/src/cli/lsp_provision/`; `engine/src/bindings/lsp.rs` |

## Bounds (reuse; don't add parallel ones)
| Bound | Value | Symbol |
|---|---|---|
| Header line / block / body | 8 KiB / 64 KiB / 64 MiB | `codec.rs` consts |
| Writer queue / reply enqueue / notify deadline | 128 frames / 1 s / 1 s + 1 s per MiB | `connection.rs` consts |
| Partial results; progress tokens; push diagnostics | 16 MiB (over → fail); 512; 256 docs × 2000 × 256 KiB | store consts |
| Request timeout; retry | 30 s → cancel + fail all; 3 × 500 ms on retryable codes | `client.rs` consts |
| stderr | 100 lines × 2000 chars, 8 KiB per line read | `client.rs` consts |
| Open documents / snippet source / didOpen | LRU 64 / 1 MB regular files only / 1 MB | `MAX_OPEN_DOCUMENTS`, `MAX_SNIPPET_SOURCE_BYTES`, `MAX_LSP_DIDOPEN_BYTES` |
| Readiness | settle 2 s, quiet 200 ms, per-language budget, max 120 s | `progress.rs`, `readiness_timeout` |
| Pool | 4 entries, 60 s idle, backoff 250 ms → 30 s | `LspPoolOptions`, `RESTART_BACKOFF_*` |
| Walk | depth 20, 200 nodes, 50 fan-out, 4 concurrent per level | `walk.rs` consts |
| Definition / aliases / page | 4 hops / 32 files read, 32 imports / 40 per page | `recovery.rs` consts |
| Probe / child memory | 3 s, process tree killed / 4096 MB (`RLIMIT_AS`; RSS watchdog on macOS) | `COMMAND_PROBE_TIMEOUT`, `process_tree.rs`; `spawn_limits.rs` |

## House patterns (copy these)
- One writer task for whole frames; a guard created before spawn fails the connection on every exit; any failure fails all pending requests.
- Cancel-safe start (`run_cancellation_safe_start` + guards that capture the runtime handle at creation).
- Clone the `Arc`, drop the guard, then `.await`. Std mutexes are never held across awaits; one async lock serializes document syncs.
- Authorize before reading: `SnippetReadPolicy` → `is_file` → bounded read.
- Typed errors all the way up: `Error::kind()` → `LspFailure::from_engine` → a public `errorCode`.

## Tests and seams
- Engine (`cargo test -p octocode-engine --all-features lsp`): `transport/connection_tests.rs` over `tokio::io::duplex` (frame split at every byte, cancel on drop, fail-all, reply-while-not-reading), `client_tests.rs` (leases, sync order, bounded reads, init params, the Node fake server), `pool.rs` (`FakeClient`, paused time).
- Runtime (`cargo test -p octocode-native --no-default-features lsp`): `R/tests.rs` (walk fake graphs, cancellation, coordinates, source cache); `R/process_tests.rs` (Node fake server: crash mid-request, acquire/stop leak loop, address-space cap).
- Still missing: a leak loop against a real server binary.

## Verify a change
1. `cargo test` for both crates, then `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`. Record baseline failures first; other sessions edit these crates.
2. `yarn build:dev` in `packages/octocode-native`, then `$OCTO lspSearch` on a Rust **and** a TS file, cold and warm. Check line/column against `sed -n`.
3. MCP: restart the server (`/mcp`) or drive a fresh `packages/octocode-mcp/dist/index.js` over stdio. A long-running server keeps its old native code.

Next: before copying an existing pattern, load `references/octocode-known-defects.md` and `references/octocode-rust-defects.md`.
