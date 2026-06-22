# Workflow — Octocode Research Loop, OQL, and Graph

Core loop: `orient → search → fetch exact evidence → prove → act`  
Carry anchors forward at every step: package, `owner/repo`, branch, path, line, PR number, `localPath`, symbol, `lineHint`.

Source docs: [`AGENT_RESEARCH_WORKFLOWS.md`](https://github.com/bgauryy/octocode/blob/main/docs/AGENT_RESEARCH_WORKFLOWS.md) · [`OQL_RESEARCH_GRAPH_FLOW.md`](https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OQL_RESEARCH_GRAPH_FLOW.md).

---

## Hard rules

1. **Read schema before raw calls.** `octocode tools <name> --scheme` first — quick-command flags and raw-tool fields are different APIs.
2. **Snippets are leads, not proof.** Re-anchor with `cat --match-string --mode none`, line ranges, AST, LSP, or history before citing.
3. **Follow returned pagination.** Never invent `next.*`, offsets, pages, local paths, or branches. Follow `charOffset`, `matchPage`, `filePage`, `next.*`.
4. **Empty ≠ absent.** Before concluding nothing was found: check spelling, branch/ref, path scope, extension filter, pagination, and provider limits.
5. **Batch independent queries; serialize dependent steps.** Multiple independent queries can go in one raw-tool call; dependent steps must wait for returned anchors.
6. **LSP requires a real `lineHint`.** Get it from `grep`/`ast`/`symbols` first — never guess.
7. **Candidate ≠ proof.** OQL `target:"research"` and `target:"graph"` return candidate evidence. Prove deletion with LSP + AST + exact reads before acting.

---

## Surface selection

| Surface | Use when | Key rule |
|---------|----------|----------|
| Quick commands (`grep`, `cat`, `lsp`, …) | Common pattern expressible as CLI flags | Prefer `--json` when another step depends on the result; preserve `location`, refs, pagination |
| OQL `search` | One typed query should route across code/content/files/structure | Use `--explain` when routing is uncertain; follow `next.*` continuations |
| Raw `tools` | Quick command can't express the needed field, pagination domain, or content selector | Always run `--scheme` first; pass schema-exact JSON only |
| OQL `target:"research"` | Broad dead-code / package-drift candidate sweep | Returns candidate rows — prove before deleting |
| OQL `target:"graph"` | Retained-by chains, relationship view, reachability | Answers "what keeps X alive?" — pair with LSP for proof-grade results |

---

## OQL patterns

### Code and content search

```bash
# Quick (auto-routes local vs GitHub)
octocode grep "registerTool" ./packages --json --compact
octocode grep "registerTool" owner/repo --json --compact

# OQL typed query
octocode search --query '{"target":"code","from":{"kind":"local","path":"src"},"where":{"kind":"text","value":"registerTool"},"view":"discovery","limit":10}' --json
octocode search --query '{"target":"content","from":{"kind":"local","path":"src/index.ts"},"fetch":{"content":{"match":{"text":"registerTool"}}}}' --json
```

### Smart reachability / dead-code / package drift

```bash
# Planning pass — understand evidence chain before sweeping
octocode search --query '{"target":"research","from":{"kind":"local","path":"."},"params":{"goal":"find unused exports, transitive dead code, unused files, and package drift","mode":"plan"}}' --json

# Analysis pass — candidate rows with verdict/why/missingProof
octocode search --query '{"target":"research","from":{"kind":"local","path":"."},"params":{"goal":"find unused exports, transitive dead code, unused files, and package drift","mode":"analyze"}}' --json
```

Result rows carry `verdict`, `why`, `retainedBy`, `missingProof`, `risk`, and `next.*`. **Treat as candidates** — prove each before deleting.

### Relationship graph / retained-by chains

```bash
# "What keeps candidate-dead exports alive?"
octocode search --query '{"target":"graph","from":{"kind":"local","path":"."},"params":{"intent":"reachability","verdict":["candidate-dead","transitive-dead"],"direction":"incoming","includePackets":true},"itemsPerPage":25}' --json
```

Use `target:"graph"` when the question is "What keeps X alive?" or "Is the keeper itself dead?".

### `--explain` and `--dry-run`

```bash
octocode search --query '{"target":"research","..."}' --explain --dry-run --json
```

Use before a sweep when routing, materialization strategy, or predicate pushdown is uncertain.

---

## `--repo` remote-as-local shortcut

`grep`, `find`, `cat`, and `ls` accept `--repo <owner/repo[@ref]>`. Materializes the repo or subpath under `.octocode`, runs the local tool against saved files, and returns `location` (absolute path).

```bash
octocode grep "registerTool" --repo facebook/react packages/react --json --compact
octocode grep --repo owner/repo src --pattern 'useMemo($$$ARGS)' --json   # AST on remote repo
octocode find "*.test.ts" --repo owner/repo --json
octocode cat src/index.ts --repo owner/repo@main --mode none --json
```

The path argument is **repo-relative** when `--repo` is set. Reuse the returned `location` path with plain local `ls`/`grep`/`cat`/`lsp` — files stay materialized. AST/structural search on a remote repo **requires** `--repo` or a prior clone; GitHub code-search cannot evaluate AST predicates.

---

## Graph research algorithm

For dead-code, reachability, retained-by, and safe-delete questions. Each step adds facts; no single step is enough.

```
1. Structure   → ls / find
                 repo shape, package roots, manifests, source dirs, tests, generated/dist

2. Discovery   → grep text/regex
                 cheap anchors, import strings, file sets, dynamic usage clues

3. AST         → grep --pattern/--rule  OR  target:"research" native graph facts
                 declarations, imports, exports, calls, class/function shapes

4. LSP proof   → lsp references / callers / callees / callHierarchy
                 semantic identity, real reference counts, caller/callee proof

5. Graph       → target:"graph"
                 entrypoint reachability, retainedBy chains, transitive-dead pruning

6. OQL packet  → packets + why + missingProof + next
                 agent-inspectable answer with exact next file/line to inspect
```

**Evidence tiers:**

| Tier | Foundation | Proves | Cannot prove alone |
|------|-----------|--------|--------------------|
| 1 | Structure + AST + LSP + graph | Strong symbol proof, bounded retained-by | Framework/runtime behavior |
| 2 | Structure + AST + graph | Structural candidates, import/export shapes | Semantic identity, overloads |
| 3 | Structure + ripgrep | Discovery, anchors | Safe deletion |

**Safe-delete rule:** candidate ≠ safe. LSP proof + graph reachability + closed `missingProof` = deletion-grade.

**Interpreting `totalReferences:0`:** LSP found no references in its open workspace. But: references only from other dead symbols = transitive-dead evidence; references from tests/generated/config may still retain. Classify before concluding.

**Question routing:**

| Question | Path |
|----------|------|
| "What looks dead?" | `target:"research"` `mode:"analyze"` |
| "Why?" | Inspect packet `why` facts and `missingProof` |
| "What keeps it alive?" | `target:"graph"` `direction:"incoming"` |
| "Is that keeper itself dead?" | Re-query `target:"graph"` for the retained-by subject |
| "What proof is missing?" | Inspect `missingProof`; follow `next.semantic` / `next.fetch` |
| "What exact file/line next?" | Use packet `next.fetch` and `subject.uri/range` |
| "Safe to delete?" | Require: no reachable external refs + no high-severity missing proof + exact source inspection |

---

## Diagnostics and failure handling

| Signal | Meaning | Next step |
|--------|---------|-----------|
| `status:"empty"` | Query ran, nothing matched | Check scope, spelling, branch, filters; try broader query or different surface |
| `status:"error"` | Tool error (auth, rate limit, validation) | Read `errorCode`; fix call or narrow scope |
| `partialResult`, `hasMore`, char pagination | Response incomplete | Follow the advertised continuation before concluding |
| `auth` / token error | GitHub/npm data inaccessible | Check `status`; ask for token only if protected data is required |
| `rate limited` | Provider result incomplete | Narrow scope or retry later |
| `ENABLE_LOCAL` / local disabled | Filesystem/clone/LSP blocked | Use remote-only proof; offer to enable |
| `serverUnavailable` / LSP unavailable | Semantic proof inconclusive | Use AST/exact content; retry after materializing project context |
| Empty `lsp references` / `callers` | Open-file scope, not absence | Load likely consumer files first, then re-query |
| Cache hit / stale cache | Evidence may reflect cached content | Use `--force-refresh` only when freshness matters |

---

## Evidence gates

- Search snippets → discovery. Fetch exact source before claiming anything.
- AST → syntax shape. Not runtime behavior, types, or semantic identity.
- LSP → semantic proof when server is available; inconclusive if unavailable or paginated short.
- History / PR patches → intent and rationale, not current behavior.
- `target:"research"` / `target:"graph"` rows → candidates. Confirm with LSP + AST + exact reads.
- OQL `metavars` absent from output → use exact snippet/line evidence; do not fabricate captures.
- If OQL returns generic records or missing continuations for a research target → fall back to the quick command or raw tool and document the fallback in the evidence trail.

---

## Workflow defaults

| Phase | Default |
|-------|---------|
| First pass | `--concise`, path-only / `mode:"discovery"`, shallow depth |
| Reading | `matchString` or line range or `--mode symbols` before full file |
| Local search | literal / fixed-string before broad regex |
| Structural search | `--pattern` for simple shapes; `--rule` YAML for relational (`inside`/`has`/`not`) |
| LSP | search first → get real `lineHint` → pass `uri`, `symbolName`, `lineHint` |
| Remote research | package/repo/code search first; clone only when local proof is needed |
| Materialization | `cache fetch` or `clone`; capture `localPath` and continue locally |
| Reporting | Cite fetched files, PRs, package metadata, or exact local `path:line` |

---

## Docs

- [Agent Research Workflows](https://github.com/bgauryy/octocode/blob/main/docs/AGENT_RESEARCH_WORKFLOWS.md)
- [OQL Research Graph Flow](https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OQL_RESEARCH_GRAPH_FLOW.md)
- [Octocode Query Language](https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OCTOCODE_QUERY_LANGUAGE.md)
