# Workflow — Graph Research Algorithm

Read when the question is about dead-code, reachability, retained-by chains, or safe deletion. Each step adds facts; no single step is enough alone. For OQL patterns that execute these steps, see [`workflow.md`](./workflow.md).

---

## Step-by-step algorithm

```
1. Structure   → search --tree / search --search path
                 repo shape, package roots, manifests, source dirs, tests, generated/dist

2. Discovery   → search text/regex
                 cheap anchors, import strings, file sets, dynamic usage clues

3. AST         → search --pattern/--rule --lang <lang>  OR  target:"research" native graph facts
                 declarations, imports, exports, calls, class/function shapes

4. LSP proof   → search --op references / callers / callees / callHierarchy
                 semantic identity, real reference counts, caller/callee proof

5. Graph       → target:"graph"
                 entrypoint reachability, retainedBy chains, transitive-dead pruning

6. OQL packet  → packets + why + missingProof + next
                 agent-inspectable answer with exact next file/line to inspect
```

## Evidence tiers

| Tier | Foundation | Proves | Cannot prove alone |
|------|-----------|--------|--------------------|
| 1 | Structure + AST + LSP + graph | Strong symbol proof, bounded retained-by | Framework/runtime behavior |
| 2 | Structure + AST + graph | Structural candidates, import/export shapes | Semantic identity, overloads |
| 3 | Structure + ripgrep | Discovery, anchors | Safe deletion |

**Safe-delete rule:** candidate ≠ safe. LSP proof + graph reachability + closed `missingProof` = deletion-grade.

**Interpreting `totalReferences:0`:** LSP found no references in its open workspace. But: references only from other dead symbols = transitive-dead evidence; references from tests/generated/config may still retain. Classify before concluding.

## Question routing

| Question | Path |
|----------|------|
| "What looks dead?" | `target:"research"` `mode:"analyze"` |
| "Why?" | Inspect packet `why` facts and `missingProof` |
| "What keeps it alive?" | `target:"graph"` `direction:"incoming"` |
| "Is that keeper itself dead?" | Re-query `target:"graph"` for the retained-by subject |
| "What proof is missing?" | Inspect `missingProof`; follow `next.semantic` / `next.fetch` |
| "What exact file/line next?" | Use packet `next.fetch` and `subject.uri/range` |
| "Safe to delete?" | No reachable external refs + no high-severity missing proof + exact source inspection |
| "Upgrade to LSP proof?" | Follow row's `next.graph` (pre-filled `proof:"lsp"`) or run graph with `proof:"lsp"` directly |
