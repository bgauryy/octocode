# Dead Code & Transitive Check Recipe

Covers two complementary tools for finding unused code:

- **[knip](https://github.com/webpro-nl/knip)** (`npx knip`) — entrypoint-aware deletion auditor. Wins at "what should I delete now?" because it understands framework conventions, package manifests, and workspace boundaries.
- **Octocode** (`oqlSearch` / `localSearchCode` / `lspGetSemantics`) — fast candidate triage and symbol-level proof. Wins at "what looks dead, why, and what exact proof do I need before removing it?"

---

## Check 1 — Deletion audit (knip whole-repo)

Use when the question is: *"What should I delete from this repo?"*

```bash
# Requires Node 22 on this machine (Node 26 npx hits an oxc-parser binding failure)
PATH=/Users/guybary/.nvm/versions/node/v22.22.0/bin:$PATH \
  npx --yes knip@latest --reporter json > /tmp/knip-report.json

# Human-readable table (default reporter)
PATH=/Users/guybary/.nvm/versions/node/v22.22.0/bin:$PATH \
  npx --yes knip@latest
```

knip issue types and what they mean:

| Issue type | Meaning | Safe to delete? |
|---|---|---|
| `files` | File not reachable from any entrypoint | Yes, unless a framework entrypoint is misconfigured |
| `exports` | Exported symbol never imported anywhere | Yes, if the package is not published |
| `types` | Exported type/interface never used | Yes, if the package is not published |
| `nsExports` / `nsTypes` | Re-exported via `export *` but never consumed | Check consumers before removing |
| `dependencies` | In `package.json` but never imported | Safe, but verify dynamic requires |
| `devDependencies` | Dev dep never used in config/scripts | Safe |
| `unlisted` | Imported but missing from `package.json` | Add to `package.json`, do not delete |
| `binaries` | `bin` entry never executed in config | Safe to remove from `bin` map |
| `unresolved` | Import specifier that cannot be resolved | Fix the import path or add the dep |
| `duplicates` | Same export from multiple files | Remove one source |
| `enumMembers` | Enum member never referenced | Safe if enum is internal |
| `namespaceMembers` | Namespace member never referenced | Safe if namespace is internal |

**Pass condition:** zero output (or only expected noise you've explicitly ignored via `knip.json`).

Run a scoped workspace to reduce noise:

```bash
PATH=/Users/guybary/.nvm/versions/node/v22.22.0/bin:$PATH \
  npx --yes knip@latest --workspace packages/octocode
```

---

## Check 2 — Fast candidate triage (Octocode research)

Use when the question is: *"Where should I look for dead code? Show me candidates fast."*

Octocode `target:"research"` runs in ~0.2s scoped vs knip's ~4.5s, but the results are **candidates**, not deletion verdicts. It does not understand framework entrypoints deeply.

The response is **paginated**: 569 packets across 12 pages at 50 per page for `packages/octocode`. Start with page 1 to get the summary and pagination metadata, then follow the `next` field to fetch subsequent pages.

Scoped to one package — page 1 (summary + first 10 packets):

```bash
octocode search \
  --query '{"schema":"oql","target":"research","from":{"kind":"local","path":"packages/octocode"},"params":{"mode":"prove","intent":"reachability","facets":["files","symbols","dependencies","relations"],"maxFiles":5000},"itemsPerPage":10,"page":1}' \
  --json --compact
```

The response envelope carries a ready-made next-page query in `next["next.page"].query` — paste it back in to advance. Or increment `page` manually and keep `itemsPerPage` consistent.

To get only the summary without any packets:

```bash
octocode search \
  --query '{"schema":"oql","target":"research","from":{"kind":"local","path":"packages/octocode"},"params":{"mode":"prove","intent":"reachability","facets":["files","symbols","dependencies","relations"],"maxFiles":5000},"itemsPerPage":1,"page":1}' \
  --json --compact
```

The `data.summary` field is always present regardless of page — it covers the full scope, not just the returned page.

Key fields in the response:

| Field | Location | Meaning |
|---|---|---|
| `evidence.kind` | envelope | `"candidate"` = triage-grade, not deletion-safe |
| `summary.candidateUnusedExports` | `data.summary` | Exports with no cross-file refs found by graph heuristic |
| `summary.transitiveDeadExports` | `data.summary` | Exports where every referencing symbol is itself a candidate |
| `summary.unusedDependencies` | `data.summary` | Package deps with no imports found |
| `packetPage` | `data.packetPage` | `{currentPage, totalPages, totalItems, hasMore}` |
| `retainedBy` | per packet | Edges: what keeps this candidate alive — follow before deleting |
| `next.fetch` | per packet | Exact OQL call to read the file/symbol |
| `next.semantic` | per packet | LSP follow-up for reference proof |

**Response size note:** `retainedBy` edges account for ~39% of the compact payload. Each edge repeats the full `from` object even when the same file appears in dozens of packets. Use `itemsPerPage:10` or lower to keep individual page responses under 30KB.

**Interpret carefully:** whole-repo `reachableFiles` is unrealistically low (Octocode does not know all framework entrypoints), so `candidateUnusedFiles` and `transitiveDeadExports` counts are inflated. Use the research layer for *where to look*, then escalate with Check 3.

---

## Check 3 — Symbol-level proof (Octocode graph + LSP)

Use when knip or research flagged a specific symbol/file and you need proof before touching it.

The `target:"graph"` + `proof:"lsp"` path gives native AST facts (declarations, imports, exports, calls, edges) PLUS page-bounded LSP reference counts in one query:

```bash
octocode search \
  --query '{
    "schema":"oql","target":"graph",
    "from":{"kind":"local","path":"./packages/octocode/src/configs"},
    "params":{
      "goal":"check unused exports in this directory",
      "intent":"reachability",
      "facets":["symbols","files","relations"],
      "mode":"prove","proof":"lsp","proofLimit":10,
      "maxFiles":50,"subject":"<SymbolName>","includePackets":true
    },
    "page":1,"itemsPerPage":15
  }' \
  --json --compact
```

Replace `<SymbolName>` with the symbol or prefix you're investigating (e.g. `"MCP"`, `"getMCPs"`).

Proof status values and what they mean:

| `proofStatus` | Meaning | Action |
|---|---|---|
| `confirmed-by-lsp` + LSP refs = 0 | Symbol has no references anywhere in the workspace | Safe to delete if also knip-flagged |
| `conflicting-evidence` + LSP refs > 0 | LSP found references — check if they are themselves dead (see Check 4) | Run transitive check |
| `needs-framework-graph` | File-level reachability needs entrypoint policy | Run knip to decide |
| `missing-proof` | LSP server not available or file not indexed | Install/start the language server |

Native graph fields to check in each packet:

| Field | What it tells you |
|---|---|
| `nativeGraphSummary.declarations` | All declared symbols in the analyzed files |
| `nativeGraphSummary.exports` | Exported symbols with `why.source:"ast"` and `confidence:"exact"` |
| `nativeGraphSummary.calls` | Call edges — used to check if callers are themselves dead |
| `proof.lsp.refs` | Cross-file reference count from LSP; 0 = unreferenced outside the file |
| `missingProof` | What proof gap remains: `dynamic-import-unresolved`, `framework-entrypoint-unknown`, etc. |

---

## Check 4 — Transitive dead symbols

A symbol has LSP refs > 0 but is still effectively dead if **every function/file that references it is itself dead**. This is the key case where Octocode's `conflicting-evidence` and knip's `unused` agree at the logical level but Octocode can't auto-resolve it.

Manual transitive check flow:

1. Find the referencing symbols from the `proof.lsp` result (step 3).
2. Look each referencing symbol up in knip's `exports` or `types` issue list.
3. If ALL referencing symbols are themselves flagged unused by knip → the target is transitively dead and safe to delete.

Example from the benchmark — `MCPCategory` in `mcp-registry.ts`:

```
MCPCategory  →  referenced by getMCPsByCategory (knip: unused)
             →  referenced by getAllCategories   (knip: unused)
```

Both referencing functions are themselves in knip's unused-export list → `MCPCategory` is transitively dead even though LSP reports 2 references.

To look up referencing symbols using Octocode LSP directly:

```bash
# 1. Anchor: get document symbols + line numbers
#    (file is a positional arg, not --path)
octocode search \
  packages/octocode/src/configs/mcp-registry.ts \
  --op documentSymbols

# 2. Prove: references for the symbol — always pass --line to avoid inference failures
octocode search \
  packages/octocode/src/configs/mcp-registry.ts \
  --op references --symbol MCPCategory --line 25
```

> **LSP gotcha:** without `--line`, the CLI infers the line with a text search
> and may anchor to the wrong occurrence (e.g. a comment or re-export).
> Always pass the explicit line number from `documentSymbols` or `search`.

**Decision rule:** if knip flags the referencing symbols too, delete all of them together. If any referencing symbol is reachable, the target is live.

---

## Check 5 — Dependency audit

knip is the right tool for dependency hygiene. Octocode can surface `unusedDependencies` and `unlistedDependencies` as research candidates, but knip's manifest-aware analysis is more reliable.

```bash
# knip: all dependency issue types in one pass
PATH=/Users/guybary/.nvm/versions/node/v22.22.0/bin:$PATH \
  npx --yes knip@latest --include dependencies,devDependencies,unlisted,binaries,unresolved
```

Cross-check with Octocode for unlisted imports (not in `package.json`):

```bash
octocode search \
  --query '{"schema":"oql","target":"research","from":{"kind":"local","path":"."},"params":{"intent":"reachability","facets":["dependencies"]},"itemsPerPage":100}' \
  --json --compact
```

The `unlistedDependencies` and `unusedDependencies` fields from Octocode are heuristic (graph-based, no manifest cross-check). Always confirm with knip before removing a dependency.

> **Bundled-dep false positive:** knip flags `@octocodeai/octocode-engine` as unused in
> `packages/octocode` because esbuild inlines it at build time — it does not appear as a
> runtime `import` that knip can trace. When a dep is bundled (esbuild/rollup/webpack),
> its absence from import statements is expected. Cross-check with `package.json` `bundleDependencies`
> or esbuild config before removing.

---

## Check 6 — Correct comparison method

The benchmark found one systematic comparison error to avoid: **matching raw LSP ref count to knip's unused verdict without checking transitivity**.

Rules for a valid comparison:

| knip verdict | Octocode `proofStatus` | Valid interpretation |
|---|---|---|
| `unused export` | `confirmed-by-lsp` (refs=0) | Match — agree to delete |
| `unused export` | `conflicting-evidence` (refs>0) | Run transitive check (Check 4) |
| `unused export` | `needs-framework-graph` | knip wins — trust it |
| Knip says live | Octocode says candidate | Trust knip — candidate is heuristic |
| Both say dead | — | High confidence, safe to delete |

Do not compare **count totals** between tools. knip counts issues (one per symbol/file), Octocode counts packets (one per research unit, which may cover multiple symbols). Compare symbol-by-symbol using the exact `filePath` + `symbol` fields.

For timing comparisons, record wall-clock separately for each tool with `/usr/bin/time -p`.

---

## Check 7 — Deterministic re-run guard

Both tools must produce stable output across re-runs on the same code:

**knip:**
```bash
# Run twice, diff the JSON output (issue sets must be identical)
PATH=/Users/guybary/.nvm/versions/node/v22.22.0/bin:$PATH \
  npx --yes knip@latest --reporter json > /tmp/knip-run1.json
PATH=/Users/guybary/.nvm/versions/node/v22.22.0/bin:$PATH \
  npx --yes knip@latest --reporter json > /tmp/knip-run2.json
diff /tmp/knip-run1.json /tmp/knip-run2.json
```

**Octocode graph proof:**

The `proof.lsp.refs` count must be identical across re-runs for the same file and same LSP server state. If it varies, a transient LSP indexing race is present — re-run with `proofLimit` reduced to avoid partial indexing.

**What is allowed to differ:** timing columns (`durationMs`, `warm ms`). Everything else — file paths, symbol names, issue types, ref counts, packet counts — must be stable.

---

## Quick reference — all dead-code check commands

```bash
# Check 1: deletion audit, whole repo
PATH=/Users/guybary/.nvm/versions/node/v22.22.0/bin:$PATH npx --yes knip@latest

# Check 1: deletion audit, scoped package
PATH=/Users/guybary/.nvm/versions/node/v22.22.0/bin:$PATH \
  npx --yes knip@latest --workspace packages/octocode

# Check 2: fast candidate triage, whole repo
octocode search \
  --query '{"schema":"oql","target":"research","from":{"kind":"local","path":"."},"params":{"mode":"prove","intent":"reachability","facets":["files","symbols","dependencies","relations"],"maxFiles":20000},"itemsPerPage":50}' \
  --json --compact

# Check 3: symbol-level graph + LSP proof, scoped directory
octocode search \
  --query '{"schema":"oql","target":"graph","from":{"kind":"local","path":"./packages/octocode/src/configs"},"params":{"goal":"check unused exports","intent":"reachability","facets":["symbols","files","relations"],"mode":"prove","proof":"lsp","proofLimit":10,"maxFiles":50,"includePackets":true},"page":1,"itemsPerPage":15}' \
  --json --compact

# Check 4: LSP reference proof for a specific symbol
#   Step 1 — find line number
octocode search <file.ts> --op documentSymbols
#   Step 2 — prove references (--line required to avoid inference failures)
octocode search <file.ts> \
  --op references --symbol <SymbolName> --line <n>

# Check 5: dependency audit
PATH=/Users/guybary/.nvm/versions/node/v22.22.0/bin:$PATH \
  npx --yes knip@latest --include dependencies,devDependencies,unlisted,binaries,unresolved
```

---

## Tool selection at a glance

| Question | Tool | Why |
|---|---|---|
| "What should I delete right now?" | **knip** | Framework entrypoints, manifest, workspace policy, low false-positive rate |
| "Where should I look for dead code?" | **Octocode research** | 3–5× faster, typed packets, `retainedBy` / `next.*` |
| "Is this specific symbol referenced?" | **Octocode LSP** | File:line proof, cross-file refs, call hierarchy |
| "Is this export reachable from entrypoints?" | **knip** | This is knip's core model |
| "Is this symbol transitively dead?" | **Both** | knip verdict + Octocode LSP proof of referencing symbols |
| "What's keeping this candidate alive?" | **Octocode research** | `retainedBy` packet field, graph edges |
| "Are my dependencies correct?" | **knip** | Manifest-aware; Octocode dep results are heuristic |
