---
name: lsp-best-practices
description: "Use when writing, reviewing, debugging, or tuning code that talks to language servers (an LSP client), or when getting trustworthy semantic answers from lspSearch. Covers the Rust/tokio implementation (codec, cancel-safety, typed errors, tasks, tests), JSON-RPC framing, the initialize/shutdown lifecycle, server→client requests, process/connection/document/pool handles, position encoding and URIs, definition/references/call-hierarchy walks, indexing readiness, per-server settings (rust-analyzer, tsserver, gopls, pylsp/pyright, clangd, jdtls), and efficiency. Typical requests: a language server hangs or leaks, a zombie or orphaned server, cold LSP is slow, callers depth looks wrong, a wrong line or column from LSP, didOpen/$/progress/ContentModified handling, adding a server route. Not for tree-sitter/oxc syntax-tree work (that is ast-best-practices), general Rust idioms (rust-best-practices), or research that uses lspSearch once (octocode-research)."
---

# LSP Best Practices

tools: `npx octocode` / `octocode-mcp`
related-skill: `ast-best-practices`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

This skill is for LSP clients that are correct at the protocol level, leak-free, bounded, cheap across many queries, and honest about what an answer proves. It is grounded in octocode's native client (`packages/octocode-native/crates/engine/src/lsp/`, `crates/runtime/src/tools/lsp_search/`) and cross-checked against the LSP 3.17 spec, Zed, Helix, rust-analyzer's `lsp-server`, async-lsp, Serena, and server source.

Flow: `FRAME → INSPECT → APPLY → VERIFY`.
- **FRAME**: name the axis: protocol, lifecycle, handles, coordinates, primitives, walks, readiness, efficiency, a specific server, or agent usage.
- **INSPECT**: read the owning engine file and reproduce the behavior with `$OCTO lspSearch` (`node packages/octocode/out/octocode.js`) before advising.
- **APPLY**: make the smallest change in the owning layer, reusing existing bounds and guards.
- **VERIFY**: run the crate tests, rebuild, then run the real CLI/MCP path on Rust **and** TS, cold and warm.

Reports go to `<output>/lsp-best-practices/`. Scratch work goes to `<output>/tmp/lsp-best-practices/`. Advice that fits in chat stays in chat.

## Mental model
- **The server is an untrusted, slow, stateful peer.** It indexes in the background, answers out of order, sends its own requests, and may hang, flood, crash, or run repository code. Everything it returns, **including URIs we then read**, is untrusted input.
- **Every conversation has two directions.** Our requests need deadlines. Its requests need answers. Silence in either direction is a hang, not an error.
- **Handles outlive intentions.** The process, process group, pending ids, open documents, and pool slots each need one owner and a release path that runs on cancel, error, timeout, and drop.
- **Coordinates are negotiated.** LSP is 0-based, with UTF-16 columns (unless negotiated otherwise) and end-exclusive ranges. Convert once, at the boundary: octocode's public output counts lines and columns from 1.
- **Empty means unknown until the server is ready.** An answer proves only "within this server's indexed scope and configuration".

## Lobby rules: the do / don't core
1. **Frame by bytes, bound before allocating.** Byte-accurate `Content-Length`, case-insensitive headers, `read_exact`, caps on the header and body. A fault poisons the connection and fails **every** pending request at once.
2. **Nothing before the `initialize` response.** Then `initialized`, then documents. Declare only capabilities we parse. Stop with `shutdown` → `exit` → bounded wait → group kill → reap.
3. **Answer every server request** through the writer queue: `configuration` per `section`, `registerCapability` → `null` (never MethodNotFound), `workDoneProgress/create` → `null`, unknown → `-32601`.
4. **Cancel on drop, not just on timeout.** A dropped request sends `$/cancelRequest`, frees its slot, and never leaves a partial frame (one writer task, or poison on any unfinished write). Never race `read_line`/`read_exact`/`write_all` in `select!`. Keep RPC errors typed (`code` enum + `Other`), never parsed back out of strings.
5. **Negotiate position encoding** and convert byte↔UTF-16 on exactly the text sent in `didOpen`. Handle `\r\n`, `\n`, and `\r`. Compare URIs as canonical paths, never as raw strings.
6. **Gate each method on the capability**, normalize every legal result shape, treat `null` as none, and round-trip hierarchy items verbatim (including `data`).
7. **Walk breadth-first with identity keys**: `(canonical uri, selectionRange)`, edges carrying `level` and `parent`, caps on depth, nodes, and fan-out enforced in code, and `truncated` with a continuation.
8. **Every wait has a deadline** shorter than the caller's. Use the strongest readiness signal available. Retry only ContentModified/ServerCancelled, matched by code. Never cache empty results.
9. **Run no repository code by default.** Headless settings for each server (no `cargo check`, build scripts, proc-macros, or repo-writing indexes) unless the caller opts in.
10. **Reuse, don't respawn, never evict the busy.** Pool per canonical key, deduplicate starts, count leases rather than requests, and let idle and LRU eviction skip leased clients. Open documents once, batch on one anchor, pipeline under a bound.
11. **Authorize before you read.** Any path from a server response passes the read policy, `is_file()`, and a bounded read **before** its bytes are touched.

## Smart routes: load only what the current step needs
- When the question is *using* LSP for evidence (anchors, line numbers, cold start, reading walks, LSP vs grep), or designing an agent-facing LSP surface, load `references/agent-usage.md`.
- When touching framing, message kinds, error codes, cancellation, or backpressure, load `references/protocol-basics.md`.
- When changing startup, shutdown, declared capabilities, or replies to server requests, load `references/lifecycle-and-server-requests.md`.
- When a process, pending request, document, or pool slot leaks or is reused wrongly, load `references/handles.md` for the handle model and cancel-safety rules. For transport or API design copied from Zed, Helix, lsp-server, or async-lsp, or for choosing a types crate, load `references/rust-client-patterns.md`.
- When a location is off by a line or column, lands in the wrong file, or dedupe fails, load `references/positions-and-uris.md`.
- When adding an operation or normalizing results, load `references/primitives.md`. When changing definition chains, callers/callees depth, type hierarchy, or reference expansion, load `references/walking.md`.
- When results are empty or partial on a cold server, or you are changing waits or retries, load `references/readiness.md`. When a call is slow or memory grows, load `references/efficiency.md`.
- When adding a server route, touching `initializationOptions` or `workspace/configuration`, or debugging one server's behavior, load `references/servers.md`.
- When writing or reviewing the Rust itself (codec, cancel-safety, `Value` vs `RawValue`, error enums, locks, tasks, drop guards, spawning), load `references/rust-implementation.md`. When choosing tests or building a fake server, a paused-time test, or a fuzz target, load `references/testing.md`.
- When touching `octocode-native` LSP code, load `references/octocode-engine-map.md` (owners, existing bounds, tests, verify steps). Before copying an existing engine pattern or reviewing an LSP diff, load `references/octocode-known-defects.md` (protocol and tool level) and `references/octocode-rust-defects.md` (Rust level: server-URI reads, busy eviction, stringly errors, races).
- For review work, use the numbered lobby rules as the checklist and cite `file:line` for each violation. `references/references.md` lists the source behind every claim.

## Related routes
- Use `ast-best-practices` for syntax-tree work (AST edges are candidates; LSP confirms identity). Use `rust-best-practices` for spawn, kill/reap, pipes, memory caps, and async mechanics in general. Use `octocode-research` to trace upstream clients or servers. Use `octocode-eval-benchmark` to prove a latency or memory change.
