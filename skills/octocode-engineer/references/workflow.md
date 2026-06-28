# Workflow — Octocode Research Loop, OQL, and Graph

Core loop: `orient → search → fetch exact evidence → prove → act`  
Carry anchors forward at every step: package, `owner/repo`, branch, path, line, PR number, `localPath`, symbol, `lineHint`.

Source docs: [`AGENT_RESEARCH_WORKFLOWS.md`](https://github.com/bgauryy/octocode/blob/main/docs/context/AGENT_RESEARCH_WORKFLOWS.md) · [`OQL_RESEARCH_GRAPH_FLOW.md`](https://github.com/bgauryy/octocode/blob/main/docs/context/OQL_RESEARCH_GRAPH_FLOW.md).

---

## Hard rules

1. **Read schema before raw calls.** `npx octocode tools <name> --scheme` first — quick-command flags and raw-tool fields are different APIs.
2. **Snippets are leads, not proof.** Re-anchor with `npx octocode search --match-string --content-view exact`, line ranges, AST, LSP, or commit history before citing.
3. **Follow returned pagination.** Never invent `next.*`, offsets, pages, local paths, or branches. Follow `charOffset`, `matchPage`, `filePage`, `next.*`.
4. **Empty ≠ absent.** Before concluding nothing was found: check spelling, branch/ref, path scope, extension filter, pagination, and provider limits.
5. **Batch independent queries; serialize dependent steps.** Multiple independent queries can go in one raw-tool call; dependent steps must wait for returned anchors.
6. **LSP requires a real `lineHint`.** Get it from `search`/`ast`/`symbols` first — never guess.
7. **Candidate ≠ proof.** OQL `target:"research"` and `target:"graph"` return candidate evidence. Prove deletion with LSP + AST + exact reads before acting.

---

## Surface selection

| Surface | Use when | Key rule |
|---------|----------|----------|
| Current commands (`search`, `unzip`, `clone`, `cache fetch`) | Common pattern expressible as CLI flags | Prefer `--json` when another step depends on the result; preserve `location`, refs, pagination |
| OQL `search` | One typed query should route across code/content/files/structure | Use `--explain` when routing is uncertain; follow `next.*` continuations |
| Raw `tools` | Quick command can't express the needed field, pagination domain, or content selector | Always run `--scheme` first; pass schema-exact JSON only |
| OQL `target:"research"` | Broad dead-code / package-drift candidate sweep | Returns candidate rows — prove before deleting |
| OQL `target:"graph"` | Retained-by chains, relationship view, reachability | Answers "what keeps X alive?" — pair with LSP for proof-grade results |

---

## OQL patterns

### Code and content search

```bash
# Shorthand (auto-routes local vs GitHub from the positional arg)
npx octocode search "registerTool" ./packages --json --compact
npx octocode search "registerTool" owner/repo --lang tsx --json

# OQL typed query
npx octocode search --query '{"target":"code","from":{"kind":"local","path":"src"},"where":{"kind":"text","value":"registerTool"},"view":"discovery","limit":10}' --json
npx octocode search --query '{"target":"content","from":{"kind":"local","path":"src/index.ts"},"fetch":{"content":{"match":{"text":"registerTool"}}}}' --json
```

### LSP semantics via OQL (`target:"semantics"`)

Use when you want LSP types (references, callers, definition, hover) through the OQL surface — especially when composing a batch query or when the `from` scope is already materialized.

```bash
npx octocode search --query '{"target":"semantics","from":{"kind":"local","path":"src/index.ts"},"params":{"type":"references","symbolName":"registerTool","lineHint":42,"groupByFile":true}}' --json
npx octocode search --query '{"target":"semantics","from":{"kind":"local","path":"src/index.ts"},"params":{"type":"callers","symbolName":"processOrder","lineHint":88,"format":"compact"}}' --json
```

Params mirror `lspGetSemantics`: `type`, `symbolName`, `lineHint`, `depth`, `groupByFile`, `format`, `includeDeclaration`. Get a real `lineHint` from `search`/`symbols` first — never guess.

### Smart reachability / dead-code / package drift

```bash
# Planning pass — understand evidence chain before sweeping
npx octocode search --query '{"target":"research","from":{"kind":"local","path":"."},"params":{"goal":"find unused exports, transitive dead code, unused files, and package drift","mode":"plan"}}' --json

# Analysis pass — candidate rows with verdict/why/missingProof/next.graph
npx octocode search --query '{"target":"research","from":{"kind":"local","path":"."},"params":{"goal":"find unused exports, transitive dead code, unused files, and package drift","mode":"analyze","intent":"symbols"}}' --json
```

Result rows carry `verdict`, `why`, `retainedBy`, `missingProof`, `risk`, and `next.*`. **Treat as candidates — results stay candidate-grade even with `mode:"prove"`** (research never runs LSP internally). Follow the row's `next.graph` continuation (pre-filled with `proof:"lsp"`) to upgrade a page of rows to LSP-proven facts:

```bash
# Upgrade research candidates to LSP proof — use next.graph from the research row
npx octocode search --query '{"target":"graph","from":{"kind":"local","path":"."},"params":{"intent":"symbols","mode":"prove","proof":"lsp","proofLimit":20},"page":1,"itemsPerPage":25}' --json
```

### Relationship graph / retained-by chains

```bash
# "What keeps candidate-dead exports alive?"
npx octocode search --query '{"target":"graph","from":{"kind":"local","path":"."},"params":{"intent":"reachability","verdict":["candidate-dead","transitive-dead"],"direction":"incoming","includePackets":true},"itemsPerPage":25}' --json

# With bounded LSP proof for current page (proof:"lsp" runs LSP reference proof per symbol packet)
npx octocode search --query '{"target":"graph","from":{"kind":"local","path":"."},"params":{"intent":"reachability","direction":"incoming","proof":"lsp","proofLimit":15},"page":1,"itemsPerPage":20}' --json
```

Use `target:"graph"` when the question is "What keeps X alive?" or "Is the keeper itself dead?".
`proof:"lsp"` adds bounded LSP reference proof for the current page's symbol packets — costs more but gives LSP-grade evidence. Rows with missing proof emit `next.graph` to upgrade the current page.

### `--explain` and `--dry-run`

```bash
npx octocode search --query '{"target":"research","..."}' --explain --dry-run --json
```

Use before a sweep when routing, materialization strategy, or predicate pushdown is uncertain.

---

## `--repo` remote-as-local shortcut

`search` accepts `--repo <owner/repo[@ref]>`. Materializes the repo or subpath under `.octocode`, runs the local lane against saved files, and returns `location` (absolute path).

```bash
npx octocode search "registerTool" packages/react --repo facebook/react --json --compact
npx octocode search src --repo owner/repo --pattern 'useMemo($$$ARGS)' --json   # AST on remote repo
npx octocode search "*.test.ts" . --repo owner/repo --search path --json
npx octocode search src/index.ts --repo owner/repo@main --content-view exact --json
```

The path argument is **repo-relative** when `--repo` is set. Reuse the returned `location` path with plain local `search --tree`, `search`, `search <file> --content-view ...`, and `search --op` — files stay materialized. AST/structural search on a remote repo **requires** `--repo` or a prior clone; GitHub code-search cannot evaluate AST predicates.

---

## Graph research algorithm

For dead-code, reachability, retained-by, and safe-delete questions, load [`workflow-graph.md`](./workflow-graph.md) — it has the six-step algorithm, evidence tiers, safe-delete rule, `totalReferences:0` interpretation, and question routing table.

---

## Diagnostics and failure handling

| Signal | Meaning | Next step |
|--------|---------|-----------|
| `status:"empty"` | Query ran, nothing matched | Check scope, spelling, branch, filters; try broader query or different surface |
| `status:"error"` | Tool error (auth, rate limit, validation) | Read `errorCode`; fix call or narrow scope |
| `partialResult`, `hasMore`, char pagination | Response incomplete | Follow the advertised continuation before concluding |
| `serverUnavailable` / LSP unavailable | Semantic proof inconclusive | Use AST/exact content; retry after materializing project context |
| Empty semantic `references` / `callers` | Open-file scope, not absence | Load likely consumer files first, then re-query |

---

## Evidence gates

- Search snippets → discovery. Fetch exact source before claiming anything.
- AST → syntax shape. Not runtime behavior, types, or semantic identity.
- LSP → semantic proof when server is available; inconclusive if unavailable or paginated short.
- History / PR patches → intent and rationale, not current behavior.
- `target:"research"` / `target:"graph"` rows → candidates. Confirm with LSP + AST + exact reads.
- OQL `metavars` absent or returns generic records → fall back to `search` shorthand or a raw tool; do not fabricate captures.

---

## Docs

- [Agent Research Workflows](https://github.com/bgauryy/octocode/blob/main/docs/context/AGENT_RESEARCH_WORKFLOWS.md)
- [OQL Research Graph Flow](https://github.com/bgauryy/octocode/blob/main/docs/context/OQL_RESEARCH_GRAPH_FLOW.md)
- [Octocode Query Language](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_QUERY_LANGUAGE.md)
