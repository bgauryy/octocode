# Octocode Query Language — Implementation & Improvement Plan

**Single source of truth for OQL sequencing + the agentic/engine improvement
backlog.** This document consolidates the former
`OCTOCODE_QUERY_LANGUAGE_PLAN.md` (implementation sequencing) and
`OQL_AGENTIC_IMPROVEMENT_PLAN.md` (research-backed gap/opportunity backlog) into
one plan. The language *contract* remains separate and authoritative:
https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OCTOCODE_QUERY_LANGUAGE.md

**Status validated 2026-06-22** against a local `support-OQL` build (`octocode`
v2.0.0 from `packages/octocode/out`, **not** the brew v1.5.3 on `PATH`). Every
"verified" line below is a live `octocode search` / `octocode <cmd>` result or a
source read, not an assumption.

---

## 1. What is live today (verified)

The OQL engine (schema, normalizer, capability registry, planner/explain,
execution adapters, materialization lane, result envelope) is implemented in
`packages/octocode-tools-core/src/oql` and surfaced as `octocode search`.

`octocode search --scheme` reports:

```
activeTargets:   code, content, structure, files, semantics, repositories,
                 packages, pullRequests, commits, artifacts, diff, research,
                 graph, materialize        (14)
reservedTargets: fixes, dataflow
sources (from):  { kind:"local" } | { kind:"github" } | { kind:"materialized" } | { kind:"npm" }
```

Verified behaviors:

- `octocode search --query <json>`, `--scheme`, `--explain` all work.
- Reserved targets reject cleanly: `target:"dataflow"` → `unsupportedTarget`.
- Unknown targets/sources reject cleanly: `target:"impact"` and
  `from:{kind:"index"}` → `invalidQuery` (not in any enum).
- `research` reachability now works on **non-JS** corpora — the Rust engine dir
  returns `files=46 symbols=133 candidateExports=18 transitiveDead=115`
  (this is the G3 fix, see §4).
- `research mode:"prove"` requires `params.intent` (deterministic proof lane);
  research rows are compact and page via `pagination` / `next.page`.
- A live **`graph` target** (`smartOqlGraph` / `queryRelationshipGraph`) answers
  relational questions over a fresh corpus, returning `evidence:candidate`
  (e.g. `graph:reachability`). This is freshness-first relational analysis — not
  a precomputed index (see §6).

**Still open at the contract/planner level** (from the original plan, still
accurate): quick-command lowering into OQL, reusable structural rule refs + rule
validation, richer structural constraints, full `controls.budget` enforcement,
and the reserved `fixes`/`dataflow` targets.

---

## 2. Decision summary & layer ownership (unchanged, still correct)

OQL ships as a typed query language compiling into existing Octocode tool
runners. Priorities: planner correctness, capability diagnostics, materialization
safety, continuation data — over surface area.

**Shipped package decision:** schema/types/normalizer/planner co-locate in
`packages/octocode-tools-core/src/oql`, exported from `oql/index.ts`; `--scheme`
served from the same module (`schemeText.ts`). Migration to
`@octocodeai/octocode-core/oql` (or a standalone
`@octocodeai/octocode-query-language`) remains the plan once a second consumer
needs standalone OQL validation. Any standalone package must stay pure (Zod,
types, normalize, diagnostics, examples) — no Octokit, MCP SDK, FS, native
engine, or interface code.

| Layer | OQL responsibility |
|---|---|
| `@octocodeai/octocode-core` | Public descriptions / command-tool text (migration target for schema/types/examples). |
| `packages/octocode-tools-core/src/oql` | Schema, types, normalizer, capability registry, planner/explain, adapters, execution routing, materialization, result envelope, provenance, `--scheme` text. |
| `packages/octocode-engine` | Native primitives only: ripgrep, structural AST, minify/content, LSP anchoring, binary/archive, secret/path-safe. |
| `packages/octocode` | Thin CLI parse/render. `search` accepts OQL; quick commands lower to OQL in Expansion. |
| `packages/octocode-mcp` | Thin MCP registration; exposes OQL schemas from core, delegates execution to tools-core. |
| `packages/octocode-vscode` | Consumer only; calls CLI/MCP surfaces. |

Dependency rule: `interfaces -> tools-core -> core/oql contract + engine primitives`.

---

## 3. Capability tracks — status

### Foundation: Universal Local + GitHub Code Research — **SHIPPED**

Canonical grammar (`schema`, `target`, `from`, `scope`, discriminated
`where.kind`, `materialize`, `fetch`, `select`, `view`, `controls`, pagination,
`explain`); strict unknown-field rejection; short-form normalization; impossible
states rejected early; targets `code`/`content`/`structure`/`files`; local +
GitHub + materialized sources; text/fixed/regex/PCRE2/exact/path/structural
lanes; remote-as-local materialization for bounded proof; full result envelope
and diagnostics. **All verified live.**

### Expansion: Universal Research Surface + Reusable Rules — **MOSTLY SHIPPED**

Shipped: `repositories`, `packages`, `pullRequests`, `commits`, `artifacts`,
`diff`, `semantics` (LSP), `research`, `graph`, `materialize` targets, all
active. **Open:** quick-command lowering (`grep`/`cat`/`ls`/`find`/`lsp`/`repo`/
`pkg`/`pr`/`history`/`binary`/`unzip`/`clone`/`diff` still on their own
implementations), reusable structural rule registry (`structuralRef` + named
ids + tests), rule validation surface (`--validate-rule`), and full
`controls.budget` enforcement beyond hard safety caps.

### Reserved: Fixes And Dataflow — **RESERVED (correctly rejecting)**

`fixes` and `dataflow` return `unsupportedTarget`. Dry-run structural fixes and
candidate/engine-backed dataflow remain gated until an engine returns
patches/traces. Honesty rules unchanged: never report `flowsTo`/`dead:true`
without proof; never mutate from a search command.

---

## 4. Improvement backlog — gaps (G) with verified status

Synthesized from web research on AST/LSP/code-search/agentic-retrieval prior
art, engine source reads, and live runs. Status column is the 2026-06-22 recheck.

| # | Gap | Lane | Verified status (2026-06-22) |
|---|---|---|---|
| G1 | Structural results unranked (pure document order) | AST | ❌ **open** — `pattern:"fn $NAME($$$){...}"` over engine src returns matches in file order; test fns sort above real decls |
| G2 | No-anchor patterns/rules parse every file and explode | AST | ❌ **open** — `rule:{kind:"call_expression"}` still floods document-order matches across all candidate files |
| G3 | `research`/reachability JS/TS-only | research | ✅ **FIXED** — Rust engine dir returns `files=46 symbols=133 transitiveDead=115` (was `0`) |
| G4 | Reachability is token-appearance, not proof | research/LSP | ⚠️ **partial** — packets are `candidate`; `mode:"prove"` now requires `intent` and routes to semantic continuations, but auto-proof step still manual |
| G5 | No relational lane (every "who calls X" needs live LSP) | LSP | ⚠️ **partly closed** — live `graph` target answers relational queries over a fresh corpus; still no *precomputed* / *cross-repo* index |
| G6 | Ranking signal computed (`classify.rs score_hint`) but not exposed | ripgrep | ❌ **open** — results not relevance-ordered across files |
| G7 | No proof-class on result rows | protocol | ⚠️ **partial** — envelope `evidence:candidate|proof` exists; per-row `matchKind`/`edgeResolution` not yet emitted |

**The strategic shift since the proposal was written:** the team chose a
**live `graph` target** (freshness-first relational analysis, no persisted
store) over the proposed persistent **`octocode ingest` SQLite index**. This
already closed G3 and partly G5 *without* a cache. The remaining open question
(§6, O5) is whether to also add the optional precomputed/cross-repo index, or
keep relational queries fully freshness-first.

---

## 5. Ranked opportunities (O) — ROI vs effort, with status

| # | Opportunity | ROI | Effort | Lane | Fixes | Status |
|---|---|---|---|---|---|---|
| O1 | Field-weighted re-rank + token-budget top-k | ★★★★★ | S | ripgrep | G6 | ❌ open |
| O2 | Proof-class (`matchKind`/`edgeResolution`) on result rows | ★★★★★ | S | protocol | G7 | ⚠️ partial (envelope-level only) |
| O3 | Structural ranking via `classify.rs` kind hints | ★★★★ | S | AST | G1 | ❌ open |
| O4 | BitSet `CandidatePlan` + multi-literal anchor | ★★★★ | M | AST | G2 | ❌ open |
| O5 | `octocode ingest` → SQLite graph (incremental) | ★★★★★ | L | new | G3,G4,G5 | ❌ not built — **superseded in part by live `graph` target; see §6** |
| O6 | Reachability as graph query w/ roots + confidence | ★★★★ | M | research | G3,G4 | ✅ effectively done via `research`+`graph` (multi-lang reachability live) |
| O7 | `target:"impact"` blast-radius | ★★★★ | M | new | G5 | ❌ not built — `impact` → `invalidQuery`; partly covered by `graph` relations |
| O8 | LSP proof-upgrade + SCIP cross-repo IDs | ★★★ | L | LSP | G4,G5 | ❌ open |
| O9 | Fix metavar flow + bound `$$$` backtracking | ★★★ | M | AST | — | ❌ open (engine) |
| O10 | Structural verify-loop tool + docs-in-context | ★★★★ | S | ergonomics | — | ❌ open (no `--validate-rule`) |
| O11 | Stack-graphs offline nav (technique, not the archived crate) | ★★★ | XL | LSP | G5 | ❌ open |
| O12 | Cost-based routing (CBO over capability routing) | ★★ | M | protocol | — | ❌ open |
| O13 | FPP: unified `.octocode.json` sidecar + 2-tier (TTL→SHA/integrity) revalidation + unified GC | ★★★★ | M | cross-cutting | stale clones/trees, provenance, disk budget | ❌ not built — cache.ts still TTL-only `octocode-clone-meta` |

---

## 6. The graph index decision (`octocode ingest`) — open, needs a call

The original proposal (O5/O7, `from:{kind:"index"}`, `$HOME/.octocode/db`) is
**verified not built**: no `ingest` command, no `better-sqlite3` dependency, no
`db/` store (`cache status` shows only clone/tree/binary/unzip), `from:{kind:
"index"}` and `target:"impact"` both `invalidQuery`.

Since the proposal, a **live `graph` target** shipped that answers relational
questions over a *fresh* corpus (no persisted index). That already delivers the
freshness-first half of the proposal's "keep freshness default, add routed
index" thesis. The remaining decision:

**Option A — keep relational queries freshness-only.** No persisted store; the
`graph` target re-analyzes per query. Wins: zero staleness, no native-packaging
risk (`better-sqlite3` is another native binary in a repo with a known embedded
-Node addon-loading issue), no privacy/GC surface. Loses: re-pays analysis cost
each session; no cross-repo "go-to-definition"; relational queries stay O(corpus
scan), not sub-ms.

**Option B — add the routed SQLite index (the original O5 design).** Home-scoped
`$HOME/.octocode/db/index.sqlite`, per-file `mtime+XXH3` freshness, 6-strategy
confidence cascade, `better-sqlite3` in tools-core, engine adds one
`extractGraph(path)→{nodes,edges}` napi fn. Adds `from:{kind:"index"}` source +
`target:"impact"` + `staleIndex`/`indexMissing`/`indexRootNotAllowed`/
`dynamicEdgeUnresolved` diagnostics + `next.ingest` continuation. Reuses the
existing `cache` command family (`cache status` gains the graph store, `cache
clear --graph`). Wins: sub-ms relational + cross-repo + token collapse (prior
art: Codebase-Memory ~10× fewer tokens, ~1.2s incremental). Costs: native
packaging gate, privacy filtering of home-scoped roots, unified GC, ~10–13 oql
files + new `oql/graph/` module.

If Option B is chosen, it must land as an explicit contract change first:
`kind:"index"`/`target:"impact"` added to enums + capability registry, Zod,
scheme text, normalizer, planner, adapters, CLI specs, docs, and tests
*together* — not piecemeal. Until then they correctly reject.

**Recommendation:** ship the cheap AST/protocol wins first (O1–O4, O10), then
decide A vs B with real query-profile data from the live `graph` target. The
graph target is the natural place to measure whether the index is worth its
operational cost.

---

## 7. Suggested sequencing

- **Phase A — agent-felt wins, cheap (days):** O1, O2 (per-row proof-class),
  O3, O10. Re-rank + proof-class + structural ranking + verify-loop. Cuts token
  cost and stops grep-as-proof.
- **Phase B — AST parity (weeks):** O4, O9. BitSet/multi-anchor prefilter,
  metavar flow, backtracking bound → ast-grep-grade structural; closes G1/G2.
- **Phase C — Expansion completion:** quick-command lowering (with parity gates),
  reusable structural rule registry + `--validate-rule`, `controls.budget`
  enforcement.
- **Phase D — the index decision (§6):** measure on the live `graph` target;
  if justified, build Option B (O5 → O6 cross-repo → O7 `impact`) behind a
  native-packaging gate.
- **Phase E — semantic depth & cross-cutting:** O8 (LSP proof-upgrade, SCIP
  cross-repo), O11 (offline stack-graphs technique), O12 (cost-based routing),
  O13 (FPP unified sidecar + SHA revalidation).
- **Reserved (gated):** dry-run `fixes`, candidate then engine-backed `dataflow`.

---

## 8. Guardrails (do not regress)

- OQL stays a typed object; no string DSL; no free-text schema fields.
- No predicate disappears during planning (pushed + residual + routed == all);
  `--explain` must show canonical OQL.
- Provider filters are not proof unless the provider can evaluate them; GitHub
  code search can be unindexed — diagnostics must say so.
- AST proves syntax shape; LSP proves semantics; never conflate. LSP
  availability is runtime-dependent, not absence.
- Materialization must be bounded and explicit; never clone broad org/user
  scopes; keep cache state visible.
- Freshness-first stays the default. Any future index is **routed and
  timestamp-checked**, never silently authoritative (`staleIndex`); graph
  answers are `candidate` unless LSP/OXC-proven; reachability/impact never emit
  `dead:true` — only "no static caller above confidence C from roots R" with
  `dynamicEdgeUnresolved`.
- Home-scoped artifacts (clones, any future index) re-check
  `ENABLE_LOCAL`/`ALLOWED_PATHS` on every read (`indexRootNotAllowed`); never
  let one session infer another repo exists.
- Secret sanitization + path validation contracts survive every adapter; the
  octocode home dir is excluded from search/ingest and never flagged as a
  secret.
- Budget exhaustion is visible as diagnostics/continuations, never silent loss.
- Current command schemas remain the live compatibility contract until OQL
  parity tests pass; do not remove legacy quick commands/raw tools until
  quick-command lowering has parity gates.

---

## 9. Verification gates (minimum)

1. Schema: every Foundation contract example parses.
2. Planner snapshots: every active target/source/predicate family routes as
   expected (`PUSHDOWN`/`RESIDUAL`/`ROUTE`/`UNSUPPORTED`).
3. Foundation parity: local text/regex/PCRE2/structural/content/files/tree,
   GitHub code/content/tree, GitHub remote-as-local proof.
4. Expansion parity: LSP, repo/npm search, PR/history, binary/archive, diff,
   research, graph, materialization, quick-command lowering.
5. Reserved proof parity: dry-run fixes never mutate; candidate dataflow never
   emits engine-proof relations; engine-backed dataflow carries trace +
   truncation state.
6. Materialization safety: bounded route works; unbounded fails.
7. Pagination: every domain (result/match/char/archive/binary/PR) returns a
   typed continuation.
8. Diagnostics distinct: `zeroMatches`, `unsupportedPredicate`,
   `requiresMaterialization`, `materializationFailed`, `partialResult`,
   `contentTruncated`, `matchTruncated`, `parserFailed`, `lspUnavailable`,
   `unsupportedSemanticOperation`, `rateLimited`, `staleCache`, `sanitized`,
   `providerUnindexed`, `partialWorkspace`, `capabilityDegraded`, `partialParse`,
   `unsupportedLanguage`, `candidateOnly`, `dataflowBackendUnavailable`,
   `pathTruncated` — plus (if index ships) `staleIndex`/`indexMissing`/
   `indexRootNotAllowed`/`dynamicEdgeUnresolved`/`patternTooAmbiguous`.
9. CLI/MCP parity: same OQL schema/behavior from both surfaces. **Note:** no OQL
   MCP tool is registered today (octocode-mcp registers the 13 raw tools only) —
   decide CLI-only vs a thin delegating MCP tool before claiming MCP OQL exists.
10. Eval coverage in `docs/OCTOCODE_EVALS.md`: schema, normalizer, planner,
    materialization, `search`, quick-command lowering, raw-tool compat,
    structural rules, budgets, graph/research, fixes, dataflow.

---

## 10. References

- OQL contract: https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OCTOCODE_QUERY_LANGUAGE.md
- CLI reference: https://github.com/bgauryy/octocode/blob/main/docs/cli/REFERENCE.md
- Engine support matrix: https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/benchmark/SUPPORT.md
- Eval tracking: https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_EVALS.md

### Evidence base for the improvement backlog (§4–§6)

Prior art (confidence): ast-grep 2-stage prefilter `potential_kinds` BitSet
(~11×) `strong`; Semgrep regex/boolean prefilter `strong`; SCIP
`(package,version,descriptor)` cross-repo IDs `strong`; stack-graphs algorithm
(crate archived Sep 2025 — technique only) `strong`/`moderate`; Codebase-Memory
(arXiv 2603.27277: tree-sitter→SQLite, ~10× tokens, 90% parity, 6-strategy
cascade, ~1.2s incremental) `strong`; SQLite CTE perf (blast-radius 0.3ms,
dead-code 150ms on 196k edges) `moderate`; KuzuDB archived Oct 2025 / CozoDB
stale — SQLite is the converged format `strong`; `better-sqlite3` vs Node
`node:sqlite` (latter runs FTS5+CTE on Node v22.22 but prints
`ExperimentalWarning`) `strong` (local). Code-grounded gaps G1–G7 are verified
by live `octocode search` runs (2026-06-22, local v2.0.0 build).
