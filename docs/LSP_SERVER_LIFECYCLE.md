# LSP Server Lifecycle — Indexing, Cold Start, and Pool Management

**Source-verified 2026-06-24 against `packages/octocode-engine/src/lsp/` and `packages/octocode-tools-core/src/tools/lsp/semantic_content/`.**

---

## Two-Layer Architecture

Octocode uses two completely different technologies that complement each other:

| Layer | What it is | Speed | Semantic? | Engine |
|---|---|---|---|---|
| **Tree-sitter / OXC** | Parser embedded in the binary | Sub-ms per file | No — syntax only | `structural/`, `search/`, `grammar.rs` |
| **LSP** | IPC to an external language server | Cold: 1–120s; warm: <100ms | Yes — cross-file | `lsp/client.rs`, `manager.ts`, `semantic_content/execution.ts` |

Tree-sitter answers "what does this code look like?" — shapes, boundaries, calls, imports.
LSP answers "what does this symbol mean?" — definition identity, all usages, call graph.

`documentSymbols` is special: it can use native JS/TS extraction, a Markdown heading outline, or LSP. JS/TS `references` also has a native same-file fallback when no server is available. Cross-file identity and the other semantic operations (`definition`, `references` beyond same-file JS/TS, `hover`, `callers`, `callees`, `callHierarchy`, `typeDefinition`, `implementation`, `workspaceSymbol`, `supertypes`, `subtypes`, `diagnostic`) require LSP.

---

## What "Indexing" Means — Per Server

Indexing is the server's startup work: read every project file and build an internal model (type graph, symbol table, cross-file references) before it can answer semantic queries correctly.

| Server | Language | What it indexes | Cold-start | Uses `$/progress`? |
|---|---|---|---|---|
| `typescript-language-server` | TS/JS | Lazy per-file; tsconfig graph | < 1s | No |
| `pylsp` | Python | Jedi/Rope analysis | 2–5s | No |
| `gopls` | Go | `go list -json ./...`, full module type-check | 3–15s | Yes |
| `rust-analyzer` | Rust | `cargo metadata` → crate graph → name resolution → type inference → macro expansion | **5–60s** | Yes (multiple waves) |
| `clangd` | C/C++ | Per-file (needs `compile_commands.json`); no whole-project | Fast/file | No |
| `jdtls` | Java | JVM startup + Eclipse JDT full workspace compilation | **30–120s** | Yes |

rust-analyzer is the hardest case. It emits multiple sequential `$/progress` waves:
1. `"Loading proc-macros"` — loads procedural macro crates
2. `"Indexing"` — name resolution for all crates
3. `"Building CrateGraph"` — type inference
4. `"Cache Priming"` — warm the query cache

A query that arrives between waves may get wrong/empty results.

---

## Current Implementation

### Pool Architecture

`packages/octocode-engine/src/lsp/lspClientPool.ts` holds a `Map<key, PoolEntry>` where each entry is a started `LSPClient` plus an idle timer.

```
LspClientPool (idleTimeoutMs = 60s, configurable via OCTOCODE_LSP_POOL_IDLE_MS)
  key: "$serverId_or_languageId\0$workspaceRoot"  (one slot per server/language × workspace pair)
  hit: return cached client, reset idle timer
  miss: factory() → start server → return client
  evict: idle timer fires → client.stop()
```

One pool per process, meaning an MCP server session keeps warm servers across tool calls within the same session.

### ProgressTracker

`packages/octocode-engine/src/lsp/json_rpc.rs`:

```
SETTLE_MS = 2_000    // wait this long for the first $/progress begin
QUIESCE_MS = 200     // after count reaches 0, wait this long for any follow-up wave
total cap: caller's timeout_ms (LSPClient default 45_000; pooled manager passes a language profile)
```

Two-phase logic:
1. **Settle**: if no `$/progress begin` arrives within SETTLE_MS, treat the server as ready.
2. **Drain + quiesce**: wait for all tokens to end, then wait QUIESCE_MS; if new tokens start, repeat.

---

## Six Ideas — Evaluated Against Source

### Idea 1: Server persistence across calls

**Status: Implemented** via `LspClientPool` (60s idle timeout).

Pool resides in the Node.js process. A long-lived MCP server process keeps warm servers across tool calls to the same workspace. Separate one-shot CLI invocations start separate processes, so they do not share a pool with each other. The 60s idle timer can be extended with `OCTOCODE_LSP_POOL_IDLE_MS`.

**Current cold-start gate**: the pool factory calls `waitForReady` for languages that emit `$/progress`, so the first semantic query waits for known indexing waves before it runs. Languages that do not use `$/progress` skip this wait to avoid a fixed 2s settle penalty.

---

### Idea 2: Per-language timeout profiles

**Status: Implemented** in `manager.ts`.

Before: one 45s cap for all servers — too long for clangd (per-file, always fast), too short for jdtls (JVM + full compilation, can take 120s).

After: per-language table in `manager.ts`:

| Language | Timeout | Rationale |
|---|---|---|
| TypeScript/JS | 0 (skip `waitForReady`) | No `$/progress`; answers queries immediately after handshake |
| Python | 0 (skip) | pylsp doesn't use `$/progress` |
| C/C++ | 0 (skip) | clangd per-file; no workspace progress |
| Bash/shell | 0 (skip) | bash-language-server no progress |
| Script/data languages | 0 (skip) | JSON/YAML/TOML/HTML/CSS servers have no indexing |
| Go | 15_000 | `go list` + module type-check, usually <10s |
| Rust | 60_000 | cargo metadata + multi-crate analysis |
| C# | 30_000 | OmniSharp workspace |
| Swift | 30_000 | sourcekit-lsp |
| Kotlin | 60_000 | kotlin-language-server full compilation |
| Java | 120_000 | jdtls JVM startup + Eclipse compilation |
| Elixir | 30_000 | workspace analysis via ElixirLS |
| Erlang | 30_000 | workspace analysis via Erlang language server |

`PROGRESS_LANGUAGES` currently contains `go`, `rust`, `java`, `kotlin`, `swift`, `csharp`, `elixir`, and `erlang`. Servers outside that set skip the `waitForReady` call entirely. For them, `waitForReady` would always burn SETTLE_MS (2s) waiting for a progress event that never comes.

---

### Idea 3: Progress streaming to caller

**Status: Open.**

Currently, when `waitForReady` times out or a query lands during indexing, the response says "may still be indexing" but doesn't show which progress tokens are still active or how far along they are.

What would be needed:
- Expose `active progress tokens` from `ProgressTracker` to the caller.
- Thread the indexing state into the response envelope as `lsp.indexingStatus`.
- Require protocol changes in tools-core response shaping.

**Effort**: M. Not yet implemented; useful but not blocking.

---

### Idea 4: documentSymbols always-fast guarantee

**Status: Partially implemented.**

`nativeDocumentSymbols()` in `execution.ts` uses OXC to extract JS/TS symbols without any server. JS/TS `documentSymbols` now prefers native OXC when it produces symbols, even if a language server is available, making the common file-outline path server-free.

Markdown files also have a heading-outline fallback that emits `documentSymbols` with `lsp.source: "markdown"`. For non-JS/TS code languages (Rust, Go, Python, Java, C++), `nativeDocumentSymbols` returns null — there is no tree-sitter based symbol extractor. These still go through the server.

**Remaining gap**: Tree-sitter symbol extraction for compiled languages would enable always-fast documentSymbols for all 30+ supported grammars. Open as a future feature.

---

### Idea 5: Explicit `waitForIndexing` param

**Status: Open.**

Currently there's no way to opt into a longer wait. A query either uses the pool timeout or gets `noLocations`.

What would be needed: add `waitForIndexingMs?: number` to `LspGetSemanticsQuery`. When set, the pool `waitForReady` timeout for that query is extended beyond the language default. Useful for CI or one-shot analysis jobs where user doesn't mind waiting 120s.

**Effort**: S. Low priority while the pool already waits for the configured progress languages.

---

### Idea 6: SCIP/precomputed index

**Status: Open.**

rust-analyzer, gopls, and others can produce a SCIP index (Sourcegraph Code Intelligence Protocol) — a serialized version of the symbol/reference graph that can be loaded instantly without re-analyzing. This would eliminate cold-start entirely.

**Effort**: XL. Requires SCIP producer tooling, a SCIP consumer in the engine, and a cache management layer. Lower priority after Ideas 1–4 are solid.

---

## Summary Table

| Idea | Status | Source / next |
|---|---|---|
| 1. Server pool | ✅ Done (60s idle, `OCTOCODE_LSP_POOL_IDLE_MS`) | Pre-existing |
| 1a. Pool factory calls `waitForReady` | ✅ Done for progress languages | `manager.ts` |
| 2. Per-language timeout profiles | ✅ Implemented | `manager.ts` |
| 3. Progress streaming to caller | 🔲 Open | Future |
| 4. documentSymbols native-first (JS/TS) | ✅ Implemented | `semantic_content/execution.ts` |
| 4a. documentSymbols Markdown heading fallback | ✅ Implemented | `semantic_content/execution.ts` |
| 4b. documentSymbols native for compiled langs | 🔲 Open (needs tree-sitter symbol extractor) | Future |
| 5. `waitForIndexingMs` opt-in param | 🔲 Open | Future |
| 6. SCIP precomputed index | 🔲 Open | Future |
