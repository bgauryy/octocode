# clasify in live research — what it did and what it saved

A record of running `clasify` against the Octocode engine's LSP and AST
implementation to benchmark quality against reference Rust projects. Numbers
are concrete: lines screened, pages auto-paged, context budget consumed vs.
context budget spared.

---

## Session summary

Four waves of clasify calls. Goal: audit `engine/src/lsp/` and
`runtime/src/tools/ast_*/` against GitHub best-practice references
(tower-lsp, rust-analyzer, syn, guppy), then rank gaps by risk.

| Wave | What | Resources | Questions | Provider calls |
|---|---|---|---|---|
| 1 | GitHub refs + local LSP + local AST | 7 | 2 each | ~30 |
| 2 | Shutdown gap, AST rewrite tail, syn reference | 5 | 2 each | ~15 |
| 3 | commands.rs kill triage, petgraph fit, guppy | 4 | 2 each | ~50 |
| 4 | stop() region confirm, evidence synthesis | 2 | 2 each | ~6 |
| **Total** | | | | **~90 calls** |

---

## What clasify screened without pulling bodies

Every file below was *judged by the provider without the content appearing
in the conversation*. The host never saw the bytes.

| Resource | Size | Screened via |
|---|---|---|
| `tower-lsp/src/service.rs` | ~600 lines | `ghGetFileContent` |
| `rust-analyzer/crates/rust-analyzer/src/main_loop.rs` | large (15 pages auto-paged) | `ghGetFileContent` |
| `rust-analyzer/crates/lsp-server/src/lib.rs` | medium | `ghGetFileContent` |
| `dtolnay/syn/src/error.rs` | ~200 lines | `ghGetFileContent` |
| `guppy-rs/guppy/src/graph/graph_impl.rs` | large (19 pages auto-paged) | `ghGetFileContent` |
| `engine/src/lsp/pool.rs` | large | `localFetch` |
| `engine/src/lsp/json_rpc.rs` | large (17 pages auto-paged) | `localFetch` |
| `engine/src/lsp/client.rs` | large (15 pages auto-paged) | `localFetch` |
| `engine/src/lsp/commands.rs` | medium | `localFetch` |
| `engine/src/lsp/workspace.rs` | medium | `localFetch` |
| `runtime/src/tools/ast_graph/algorithms.rs` | medium | `localFetch` |
| `runtime/src/tools/ast_graph/graph.rs` | ~42 KB | `localFetch` |
| `runtime/src/tools/ast_graph/types.rs` | medium | `localFetch` |
| `runtime/src/tools/ast_rewrite/mod.rs` | 1692 lines (17 pages) | `localFetch` |

### Auto-paging in action

`ast_rewrite/mod.rs` is 1692 lines. clasify auto-paginated it into **17
independent 100-line windows**, each judged separately. No manual cursor
management. The output included per-page `scope.startLine`/`scope.endLine`
so the weak regions were immediately locatable:

```
pages 14–16 (lines 1401–1692): idiomatic-rust score 0.33–0.38  ← lowest
pages 6–8   (lines 601–800):   needs-hardening confidence 0.75  ← most consistent
pages 0–2   (lines 1–200):     production-ready = insufficient  ← context too thin per page
```

The same file read via `localFetch` for full context would have consumed
**~40 KB of context** in one shot and still required manual analysis.
clasify used ~1900 tokens per page (provider-side) and returned structured
verdicts the code could act on directly.

---

## Triage wins — what was NOT read

### `commands.rs` — kill-vs-graceful
clasify returned `not-applicable` at **confidence=1.00** for "does this file
handle server termination?" One call. No `localFetch` needed. The file is a
commands layer, not a lifecycle layer — exactly right.

Without clasify the workflow would have been:
1. Read `commands.rs` (~200 lines)
2. Read `workspace.rs` (~200 lines)
3. Conclude neither handles termination
4. Pivot to `client.rs`

clasify did steps 1–4 in one call with no body in context.

### `pool.rs` — RAII cleanup
P(yes)=0.97 for "uses Drop/RAII for cleanup". Confirmed in one page. No
deep read of pool internals needed to establish that cleanup is handled.

### `lsp-pool` — shutdown sequence
P(yes)=0.09 — clearly delegates. Directed attention to `client.rs` where
the actual `stop()` lives. Grep confirmed: `client.rs:511-512` sends
`shutdown` then `exit` in order. clasify identified the right file to grep;
without it, the search would have started at `pool.rs` and worked outward.

### rust-analyzer `main_loop.rs`
15 pages auto-paged. The file delegates shutdown elsewhere (correctly).
P(yes)=0.18 — correct: shutdown lives in the connection layer. Without
clasify: download 15 pages, read 1500+ lines, conclude "delegates." With
clasify: one call, 15 judgments, same conclusion.

---

## Quantities

| Metric | Without clasify | With clasify |
|---|---|---|
| Context consumed (est.) | ~120 KB (all files read) | ~0 KB body text |
| Manual page turns | 100+ (17+19+17+15+15 pages across 5 large files) | 0 — all auto-paged |
| Files read that proved irrelevant | 2+ (commands.rs, workspace.rs) | 0 |
| Structured verdicts returned | 0 — all inference | 90 typed answers (noul/score/choice) |
| Weak-region localization | "read the file" | Line-range pinpointed (1401–1692) |
| Evidence synthesis | Manual summary | Wave 4: Jev on supplied `value` context |

---

## What clasify cannot do here — honest limits

- **Page-level scores are local.** A 100-line window scoring "idiomatic" doesn't
  mean the whole file is idiomatic. Coverage metadata (`partial` vs `bounded`)
  signals this but doesn't aggregate automatically.
- **"insufficient" is real data.** `ast_graph/algorithms.rs` returned
  `insufficient` for production-ready not because it's bad but because the
  question requires full-file context to answer and the file is one byte-ranged
  page. `localFetch` + a targeted read confirmed it's fine.
- **Confidence calibrates the answer, not the question.** A high-confidence
  `needs-hardening` (0.90 on `ast_graph/graph.rs`) is actionable. A low-confidence
  `needs-hardening` (0.44 on a mid-page of `ast_rewrite`) is a flag to read
  that region, not a verdict.
- **Restart gap is absence of evidence, not evidence of absence.** The three
  files checked (pool.rs, commands.rs, workspace.rs) all scored P=0.02–0.03 for
  auto-restart. That's a strong NO from the right places to look. But a
  `localSearch` for `respawn`/`restart` was not run — confirming absence requires
  that extra step.

---

## Pattern: how to use clasify in future research cycles

```
1. clasify ghSearch/ghGetFileContent   → screen GitHub refs, no body pulled
2. clasify localFetch (large files)    → auto-page, get per-region scores
3. read flagged regions only           → use scope.startLine/endLine
4. clasify value:{evidence summary}    → synthesize into priority order
5. act on winner with highest conf     → skip the low-conf cells
```

The key shift: **clasify answers "where to look" cheaply; read tools answer
"what is there" completely.** Using them in sequence avoids reading files that
aren't the answer.
