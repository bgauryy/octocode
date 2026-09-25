# Rust Client Patterns: what mature clients do

Load when designing or refactoring the transport, request API, or type layer of a Rust LSP client, or when choosing an LSP types crate. Why: Zed, Helix, rust-analyzer's `lsp-server`, async-lsp, and tower-lsp-server have already solved (or documented failing at) most transport problems. Copy the pattern instead of rediscovering the bug.

## Patterns to copy

| Pattern | Source | Why |
|---|---|---|
| Cancel-on-drop guard: armed at request creation, disarmed on response, `Weak` reference to the writer | zed `crates/lsp/src/lsp.rs` (`request_internal_with_timer`) | Covers timeout **and** caller drop. Does nothing if the server is already gone. |
| Three-way result `Result \| Timeout \| ConnectionReset` | zed `lsp.rs` | Callers can tell a slow server from a dead one |
| Pending map `Mutex<Option<HashMap>>`, `take()` by a `defer` in both I/O tasks | zed `lsp.rs` | Fails everything still waiting at once. New requests fail fast with "server shut down". |
| Bounded inbound queue (128): the reader stops reading so the OS pipe pushes back | zed `input_handler.rs` | Stops a flooding server from growing memory without bound |
| Rendezvous channels (`bounded(0)`) plus a separate message-dropper thread | rust-analyzer `lib/lsp-server/src/stdio.rs` | Real backpressure. Freeing large messages never slows the writer. |
| Queue non-init traffic in the transport until `initialize` completes | helix `helix-lsp/src/transport.rs` | Callers can't violate the "nothing before initialize" rule |
| Assign the id and enqueue synchronously, outside the future | helix `helix-lsp/src/client.rs` | Stable request order |
| Pin the `Notified` future outside the loop | helix `transport.rs` comment | Recreating `notified()` inside `select!` can lose the permit |
| Incoming-request concurrency cap (semaphore) plus cancel → abort handle | async-lsp `src/concurrency.rs` | Bounded server→client work |
| Priority: outgoing > internal > incoming | async-lsp `src/lib.rs` main loop | Backpressure against inbound floods |
| `force_shutdown` with a short grace, then `kill_on_drop` | helix `helix-lsp/src/lib.rs` | Quitting never blocks on a slow server |

## Anti-patterns they document
- **A timeout that neither cancels nor removes the pending entry** (Helix `client.rs`): the entry leaks until the server answers.
- **An id assigned inside the event loop** (async-lsp): the caller's future doesn't know its own id, so dropping it can't send a cancel.
- **Unbounded stderr capture** (Zed, Helix): bound it yourself with a ring buffer **and** a max line length.
- **Duplicate handler registration**: Zed panics on a second handler for the same method. Make it impossible, not silent.

## Types crate: prefer owning the wire format
- `lsp-types` 0.97 (gluon-lang, dormant since 2024-06) changed `Uri` from `url::Url` to `fluent_uri`, which broke every client's path↔URI code.
- rust-analyzer and tower-lsp-server moved to `gen-lsp-types` 0.11, which is generated from the official metamodel and has a feature-gated URI backend. Helix vendors a fork in which the URI is an opaque RFC 3986 `String`. Zed pins its own git fork.
- **Rule:** keep URIs as opaque strings on the wire and convert to paths yourself at one choke point. `url::Url` (WHATWG) changes percent-encoding and drive letters, and servers may reject the result. octocode builds JSON directly with `serde_json` and uses `url` only for path conversion, which avoids the crate churn. Keep it that way unless a typed layer pays for itself.

Next: for how these map onto octocode load `references/octocode-engine-map.md`. For the gaps they reveal, `references/octocode-known-defects.md`.
