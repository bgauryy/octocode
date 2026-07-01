# Finding Checks

Read this before presenting, dismissing, or acting on a finding. Every finding starts as a hypothesis. Promote it only when the evidence source can actually prove the claim.

## Confidence

- `confirmed`: two independent evidence sources agree, or one authoritative deterministic check proves the claim.
- `likely`: one source supports the claim, or the evidence is a reasoned approximation.
- `uncertain`: hypothesis, snippet, or incomplete proof path.

Search snippets, LLM judgment, and package popularity are leads. Exact reads, AST, LSP, PR/commit evidence, binary metadata, formal docs/specs, build/test/typecheck/lint, and reproducible command output can be proof.

## Proof Ladder

1. Candidate search: find possible locations, names, or symptoms.
2. Exact read: inspect the concrete anchor.
3. Shape check: AST/structural search proves code form.
4. Identity/reachability: LSP definition/references/callers/callees proves symbol relationships.
5. Independent corroboration: tests, build/typecheck/lint, history, docs/specs, or a second search shape.
6. Verdict: confirmed/likely/uncertain, with the falsifying check named.

## Common Findings

| Finding | Minimum check before reporting as real |
|---------|----------------------------------------|
| Dead export | OQL/research candidate + LSP references excluding declaration + AST/import search + broad text search |
| Safe deletion | Dead-export checks plus tests/build or explicit "not verified" confidence cap |
| Dependency cycle | Imports both directions by search/AST; for full clusters, mark native proof as pairwise unless a graph tool confirms the SCC |
| Security sink | AST/search sink + exact read + callers/source trace + guard/sanitizer check |
| Test gap | Changed/important symbol has no test references, plus exact read of nearby tests or test tree |
| Coupling hotspot | fan-in/fan-out proxies + exact read showing mixed responsibilities; mark as approximation unless measured |
| God function | exact read + callees/side effects + caller blast radius; note if it may be an intentional transaction script |
| Performance smell | exact read proves independence/hot path; tests/benchmarks only when the claim needs runtime proof |

## Dismissal

Dismiss a candidate when a stronger check contradicts it. Say why, briefly: "dismissed because LSP callers show live production use" or "dismissed because exact read shows the import is type-only and erased."
Keep dismissed candidates out of the final findings list; surface one only as a short residual-risk note when it still matters.

## Reporting

Every finding should include: claim, anchor, proof check, confidence, impact, and next action. If a deterministic check was not run, say so and keep confidence below `confirmed`.
