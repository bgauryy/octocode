# TODO

## Context budget: tool schemas and instructions

Measured 2026-09-29 with `skills-dev/octocode-context-audit` and a live MCP `tools/list` (13 tools):

| Part | Chars | Share |
|---|---|---|
| MCP server instructions | 3,807 | 5% |
| Tool descriptions | ~4,200 | 5% |
| Input schemas | ~72,000 | 89% |
| **Total `tools/list` + instructions** | **81,427 (~20k tokens)** | |

Of the schema bytes, only ~16,000 are description prose. The rest is structure: types, enums, bounds, `required`, `additionalProperties`, and `pattern:"\S"` on 57 strings.

Claude Code defers MCP tool schemas until a tool is selected, so for Claude the every-session cost is the instructions plus tool names. Clients without deferral (Cursor, others) pay the full ~20k tokens.

### Safe, no behavior change (small)
- [ ] Hoist fields repeated across variants into `$defs` refs: ghSearchHistory, artifactSearch, structureSearch, ghGetHistoryItem, lspSearch. Measured saving ~1.8k chars. Verify that the generated JSON Schema validates the same inputs (contract fixtures) and that MCP clients accept `$ref` in each variant.
- [ ] Prose pass on descriptions over 75 chars (~30 strings). They are already budget-tested and dense; expect ~1–2k chars. Keep every deciding fact: defaults, bounds, exclusivity, continuation rules.

### Contract changes (need a decision; they change the flow)
- [ ] Response paging fields (`responseCharLength`, `responseCharOffset`, `responseScope`, `responseSnapshot`) repeat on all 12 tools: ~4.5k chars. Options: keep them only on tools that emit large text pages, or document them once in the server instructions.
- [ ] `debug` repeats on every tool (~1k): consider a server-level debug switch.
- [ ] Variant-heavy tools (ghGetHistoryItem 9.2k, astSearch 8.6k, lspSearch 8.6k, clasify 8.5k, ghSearchHistory 7.9k): check whether rarely used fields can move behind a nested `options` object or a separate operation. Use transcript usage from `context-audit` before cutting.
- [ ] High error rates in transcripts: lspSearch (5 of 8 calls), ghSearchHistory and artifactSearch (~25%). Check whether schema wording causes them before trimming these tools.

### Instructions
- [x] AGENTS.md 22.6k → 14.8k chars: reference detail moved to its owning docs, and a shell→tool routing map added.
- [x] Claude memory index 20.4k → 11.1k chars.
- [ ] MCP server instructions (3.8k): the grammar inventory (505 chars) and repeated routing sentences are candidates. Keep under the core instruction budget test.
- [ ] Agents bypass Octocode ~3.6:1 (4.4k raw grep/cat/find vs 1.2k Octocode calls in 30 days). Re-run `context-audit` in two weeks to see whether the AGENTS.md routing map changes it.

### Skills
- [x] Removed 5 broken skill links; unified the divergent `octocode-eval-benchmark` and `octocode-orchestrator-local-worker` copies.
- [ ] `octocode-subagent` and `octocode-orchestrator-local-worker` overlap (local Ollama offload): keep one.

## Response size and native flow (2026-09-29)

Done:
- [x] Minimal responses by default; `debug: true` adds metadata. Each tool declares its debug-only fields, and a contract-derived guard keeps every required variant field. Measured over 18 benchmark calls: 40,293 → 34,857 chars (−13%). Local tools −12% to −54% (astSearch match −54%, symbols −39%).
- [x] ToolId resolved once per call; exhaustive routing in dispatch, GitHub and renderers; one error-row constructor.
- [x] One cursor type; clasify has its own entry; batch width is a named, measured policy.
- [x] One directory prune policy (search-safe and syntax-visible) for localSearch, structureSearch, astSearch, astRewrite, astTopology and graph ingest.
- [x] Relevance ranks declarations above comments and source above tests.
- [x] Graph facts no longer climb `parent()` inside deadlines.
- [x] Rust dead-code liveness resolves through bindings.
- [x] Clasify judgment cache (process-local, exact-request key).

Open (need a decision or a contract change):
- [ ] `next.*` continuations repeat the full `goal` + `reasoning` on every page (25–30% of small GitHub pages, more with real briefs). Option: let a continuation inherit its brief from the row that emitted it (input contract change).
- [ ] No explicit way to clear the default directory prune on all walks (needs one shared contract field).
- [ ] Import liveness is per file: a `use` inside a dead function still keeps its target alive (needs value-reference counts on import bindings).
- [ ] A shared walk-worker limit across a batch (the engine must accept a thread budget); the current width stays until that measures faster.
- [ ] ghSearchRepo rows carry every repository metadata field; consider a smaller default field set (`concise` exists).

## Open (2026-09-29 large-PR + clasify scout pass)
- astTopology Rust: `[patch.crates-io] tokio = { path = … }` redirects are not linked and produce no diagnostic (deps-flows Rust Sender check).
- ghGetHistoryItem PR inventory: add path-prefix/glob + status filters (core validation/history.ts); compact rows (omit default `status: modified`); patch window fixed at 8k (13 calls for 100k patch).
- clasify: `sufficient` preset unreliable at provider level (false "sufficient" on bare declaration) — hint only; locate `best` at walk end needs a "not found" signal (core text); goal repeated per question in payload.
- localSearch files-view continuation pages rescan the whole tree (~24s/page on 810MB repos under load).
- NASM `EXTN(name):` labels not recognized as declarations.
