# OQL Results and Evidence

How to read OQL results, interpret evidence, handle diagnostics, follow
continuations, and safely answer research and dead-code questions.

**Language reference:** [OQL_LANGUAGE_REFERENCE.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OQL_LANGUAGE_REFERENCE.md) — query anatomy, targets, predicates, params.
**Quick reference:** [OCTOCODE_QUERY_LANGUAGE.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OCTOCODE_QUERY_LANGUAGE.md) — cheatsheet, decision tree, recipes.

---

## Result Envelope

Single-query result:

```ts
interface OqlResultEnvelope {
  queryId?: string;
  queryIndex?: number;
  results: Array<OqlResultRow & { proofGrade: OqlProofGrade }>;
  pagination?: Pagination;
  next?: Record<string, OqlContinuation>;
  diagnostics: OqlDiagnostic[];
  provenance: OqlProvenance[];
  evidence: {
    answerReady: boolean;
    complete: boolean;
    kind: "proof" | "partial" | "candidate" | "unsupported";
  };
  plan?: OqlExplainPlan;
}
```

Result row kinds:

| Row kind | Fields |
|---|---|
| `code` | `proofGrade`, `source`, `path`, `line`, `endLine`, `column`, `snippet`, `metavars`, `metavarRanges`, `next` |
| `file` | `proofGrade`, `source`, `path`, `entryType`, `size`, `modified`, `next` |
| `tree` | `proofGrade`, `source`, `path`, `entryType`, `depth`, `size`, `children`, `next` |
| `content` | `proofGrade`, `source`, `path`, `content`, `range`, `contentView`, `next` |
| `record` | `proofGrade`, `recordType`, `id`, `source`, `data`, `next` |

`proofGrade` is mandatory per row: `candidate`, `text`, `structural`, `semantic`,
`graph`, or `missing`. Projection (`select`) never removes it.

Record types:

| `recordType` | Payload family |
|---|---|
| `semantics` | LSP operation result |
| `repository` | repository search result |
| `package` | package search result |
| `pullRequest` | PR result |
| `commit` | commit result |
| `artifact` | artifact inspection result |
| `diff` | diff result |
| `research` | smart research flow with summary and paged packets |
| `graph` | relationship graph with nodes, edges, facts, packets, and missing proof |
| `materialized` | materialized checkpoint |

---

## Evidence Tiers

`evidence` is part of every answer. It tells you what the result is worth.

| Signal | Agent conclusion |
|---|---|
| `answerReady:true` and `complete:true` | You may answer the query as asked. |
| `answerReady:false` | Normal — not a failure. Results above are valid; follow `next.*` for more pages, LSP proof, or content. |
| `complete:false` | Pages/proof/slices may remain. Follow `next.page`, `next.fetch`, `next.semantic`, `next.search`, `next.graph`, or another returned continuation. |
| `evidence.kind:"proof"` | Backend and OQL routing evaluated the requested semantics exactly. |
| `evidence.kind:"partial"` | Truncation, pagination, or residual checks remain. Report the gap or continue. |
| `evidence.kind:"candidate"` | Report candidates only. Do not claim absence or safe deletion. `research`/`graph` are always candidate — upgrade via `next.semantic`/`next.search`/`next.fetch`. |
| `evidence.kind:"unsupported"` | Do not answer as if the query ran. Read diagnostics and repair. |
| `zeroMatches`/`providerUnindexed` on provider search | NOT absence unless `--explain` proves exact bounded evaluation. Verify path with `--tree`, then materialize/clone/cache. |
| `providerSemanticsApproximate` | Useful discovery result, not proof. Materialize or use local/LSP proof for final claims. |
| `proofStatus:"candidate"` | Pre-proof state (no LSP run yet). Run the row's `next.graph` (proof:"lsp") to resolve it. |
| `proofStatus:"conflicting-evidence"` | LSP refs > 0 — the symbol IS retained; inspect `retainedBy`. |
| `proofStatus:"confirmed-by-lsp"` | LSP refs = 0 in the bounded workspace. Still check entrypoints, framework conventions, dynamic imports, and package/script exposure before deletion. |
| `proofStatus:"needs-framework-graph"` | LSP alone cannot prove reachability; inspect framework and entrypoint evidence. |

---

## Diagnostics

Agents must read diagnostics before answering. When a diagnostic has
`repair.suggestedQuery`, prefer that over inventing a new shape.

| Code | Meaning |
|---|---|
| `invalidQuery` | Query shape or params are invalid. |
| `unknownField` | Field is not part of OQL. |
| `unsupportedTarget` | Target is reserved or inactive. |
| `unsupportedPredicate` | Predicate cannot run on that target/source. |
| `negativeUniverseRequired` | Negation needs a complete local/materialized universe. |
| `requiresMaterialization` | Exact proof needs materialization. |
| `materializationNotAllowed` | Query needs materialization but mode forbids it. |
| `providerUnindexed` | GitHub provider returned zero rows. NOT absence — verify path, then materialize. |
| `providerSemanticsApproximate` | Provider filter is useful but not exact proof. |
| `vendorNoEquivalent` | A selected backend has no equivalent for the requested OQL intent. |
| `lossyTransform` | A backend mapping would drop or narrow part of the OQL intent. |
| `unsupportedVendorPredicate` | The backend cannot evaluate this predicate; use another target/source or materialize. |
| `responseShapeMismatch` | The backing tool returned a shape the OQL mapper did not understand. |
| `partialResult` | Result is incomplete or candidate-grade. |
| `contentTruncated` | Content was sliced. |
| `matchTruncated` | Match snippet was sliced. |
| `planTruncated` | Explain plan was capped. |
| `budgetExhausted` | Budget stopped the query. |
| `parserFailed` | Parser failed. |
| `partialParse` | Parser skipped or partially parsed inputs (often a missing literal anchor). Non-fatal. |
| `lspUnavailable` | LSP server or capability unavailable. |
| `sanitized` | Secrets or sensitive content were redacted. |
| `rateLimited` | Provider rate limit. |
| `zeroMatches` | No matches. Verify scope before treating as absence. |

---

## Continuations

Every `next.*` value is an executable OQL query — follow it rather than inventing
paths, anchors, pages, or proof queries.

```ts
interface OqlContinuation {
  query: OqlCanonicalInput;
  baseQueryId?: string;
  queryIndex?: number;
  why: string;
  confidence: "exact" | "heuristic";
}
```

| Continuation | Meaning |
|---|---|
| `next.fetch` | Read exact content or an outline. |
| `next.semantic` | Ask LSP about the symbol/file. |
| `next.search` | Run a text/regex follow-up. |
| `next.page` | Continue the primary result domain, including research packet pages. |
| `next.matchPage` | Continue per-file match pages when a file has more matches than returned. |
| `next.charRange` | Continue a content range (char windows). |
| `next.graph` | Upgrade a research or graph candidate page with bounded graph/LSP proof. |
| `next.structure` | Inspect the tree of a materialized or extracted local path. |
| `next.files` | Enumerate files in a materialized or extracted local path. |
| `next.artifactStrings` | Continue a binary/artifact strings scan. |
| `next.materialize` | Materialize the bounded corpus from the failed query. |

Continuations carry path, range, source, and reasoning already validated by OQL.
Always prefer a returned continuation over constructing your own query.

---

## Research and Graph Flows

`target:"research"` and `target:"graph"` answer reachability and dead-code
questions. Both are candidate-first — promote candidates with
`next.semantic` / `next.search` / `next.fetch` before any deletion claim.

### Recommended Two-Phase Reachability Flow

**Phase 1 — summary + first candidate packet:**

```json
{
  "target": "research",
  "from": { "kind": "local", "path": "." },
  "params": {
    "intent": "reachability",
    "facets": ["symbols", "files", "relations"],
    "mode": "analyze"
  },
  "page": 1,
  "itemsPerPage": 1
}
```

**Phase 2 — page candidate packets:**

1. Follow `next.page` for each additional packet page.
2. Run the row-level `next.graph` exactly as returned (it is page-aligned).
3. Use `params.proof:"lsp"` or the returned graph continuation to attach bounded
   LSP proof to the current packet page.
4. Follow packet-level `next.fetch`, `next.semantic`, and `next.search` for exact
   evidence.
5. Treat `answerReady:false` as normal for candidate research.
6. Only make deletion-grade claims after `proofStatus`, diagnostics, missing
   proof, entrypoints, framework conventions, dynamic imports, and package/script
   exposure all support the conclusion.

### What Research and Graph Return

Research rows (`recordType:"research"`) can include:
- `summary`, `graphCapabilities`, `nativeGraphSummary`, `graphSummary`
- `packetPage`, `packets`, `caveats`
- In `view:"detailed"`: raw arrays `manifests`, `files`, `dependencies`, `symbols`, `graphFacts`

Each packet can include:
- `subject`, `verdict`, `proofStatus`, `why`, `retainedBy`, `missingProof`, `risk`, `next`

Graph rows (`recordType:"graph"`) can include:
- `nodes`, `edges`, `facts`, `missingProof`
- `packets`, `nativeGraphSummary`, `graphCapabilities`
- `summary`, `graphSummary`, `packetPage`

### Research vs. Knip

OQL beats a single knip-style command when the agent must ask "why?" and continue
into exact proof. A dedicated knip-style tool is better for a one-shot,
framework-aware entrypoint/dependency audit.

---

## Safe Deletion Rules

For "is this safe to delete?", require all of:

1. A bounded local or materialized corpus.
2. Export/declaration inventory from regex, AST, or LSP document symbols.
3. LSP references with `includeDeclaration:false`.
4. External-reference classification (exports, package.json, scripts).
5. Transitive keeper checks — references that point only to other candidate-dead symbols.
6. File-level import/entrypoint checks.
7. Dependency and script checks for package changes.
8. Review of `diagnostics` and `missingProof`.

A good OQL answer can say:
- "candidate dead"
- "proof missing: LSP references"
- "retained by file X line Y"
- "keeper is also unreferenced"
- "safe to inspect next"
- "not safe to delete yet"

It must NOT say "delete this" when `evidence.kind:"candidate"` or
`answerReady:false`.

---

## Current Limits

**Tell agents these before research/graph work:**

- `target:"research"` is the right entry for knip-like questions, but results are candidate-first. Use `next.semantic`/`next.search`/`next.fetch` to upgrade evidence.
- `target:"graph"` is the right entry for retained-by chains. Use `params.proof:"lsp"` or `mode:"prove"` for bounded LSP proof on the current page, then keep paging until missing proof is closed.
- Native graph inventory is capability-driven across OXC/tree-sitter languages. Read `graphCapabilities`/`nativeGraphSummary` before claiming absence; missing capability is not proof of absence.
- `mode:"prove"` on `target:"graph"` is page-bounded. Unproved pages, unavailable LSP servers, paginated LSP results, dynamic imports, and framework entrypoints remain `missingProof`.
- Tree-sitter graph facts are syntax inventory; public/export hints and call edges are language-aware candidates, not semantic proof. LSP references, definitions, and call hierarchy are the proof layer.
- Structural AST search is exact only when the pattern/rule is accepted by the parser and diagnostics are clean.
- File and dependency deletion still need project-specific entrypoint, framework, script, dynamic import, and generated-file awareness.
- LSP proof is only as complete as the workspace, language server, and symbol anchor provided.

---

## Full Research Algorithm

For the complete algorithm — structure → discovery → AST → LSP → graph → packet,
evidence tiers, verdicts, graph-capability fields, language coverage, and the
question-to-field map — see the canonical research contract:

[OQL Research and Graph Flow](https://github.com/bgauryy/octocode/blob/main/docs/context/OQL_RESEARCH_GRAPH_FLOW.md)
