# Code Research

Read when `octocode-research` handles code investigation, implementation, review, refactor, architecture, dead-code, PR/local diff, binary/archive, or blast-radius work.
Before presenting, dismissing, or acting on a finding, use this workflow and proof ladder.
Evidence grades, the router, anti-patterns, and failure signals live in `references/algorithm.md`.
For local/external/debug/change/PR-review routing, load `references/workflows.md` (index) or the matching `workflow-*.md` file first, then return here for the proof ladder.

## Route

| Need | Mode | First proof path |
|------|------|------------------|
| Bug/root cause/behavior | Investigate | orient -> hypothesis map -> exact read -> AST/LSP/history |
| Implementation/refactor | Plan or Change | current contract -> blast radius -> existing pattern -> minimal patch -> tests |
| PR/local diff review | Review | changed region -> affected symbols -> tests/consumers -> ranked findings |
| Dead code/safe delete | Investigate or Review | OQL/research candidates -> LSP/AST/broad search -> tests before deletion |
| Architecture assessment | Plan | entry points -> dependencies -> fan-in/fan-out proxies -> tradeoffs |
| Binary/archive/artifact | Investigate | inspect/list/strings/extract -> local tree/search/read |

## Workflow

1. State goal, scope, and whether the user expects a patch, review, or research answer.
2. Map before reading: tree/path discovery, largest/churned files, entry points, changed files, or artifact contents.
3. Keep two hypotheses alive: likely explanation plus alternate; name the check that would disconfirm each.
4. Read exact slices around anchors; use `--json` when later steps need paths, refs, pages, or line numbers.
5. Use AST for code shape and LSP for symbol identity, callers, references, and blast radius.
6. For code changes, find an existing local pattern before editing; make the smallest scoped change that follows from the evidence.
7. Verify with the declared check: targeted test/build/typecheck/lint, AST/LSP rerun, CLI/API smoke, exact read, or history proof.
8. If verification fails, keep the failure visible, re-read the failing path, patch only the cause, or report blocked with the exact gap.
9. Report confidence as `confirmed`, `likely`, or `uncertain`; never upgrade a snippet to proof without a check.

## Review And Change Gates

Ask before continuing when the next step changes a public contract, crosses packages/layers, deletes/renames shared exports, affects many consumers, requires a product/architecture tradeoff, or conflicts with current evidence.

For a review, lead with findings ordered by severity. Each finding needs `file:line`, impact, evidence, and a fix path. If there are no findings, say that and name residual test/risk gaps.

For a code change, do not claim success until the verification command actually ran. If verification is impossible, say why and keep confidence below `confirmed`.
For a failed change, report `attempted patch -> failing check -> next proof step`; do not silently broaden the patch.

## Proof Ladder

Every finding starts as a hypothesis. Promote it only when the evidence source can actually prove the claim.

1. Candidate search: find possible locations, names, or symptoms.
2. Exact read: inspect the concrete anchor.
3. Shape check: AST/structural search proves code form.
4. Identity/reachability: LSP definition/references/callers/callees proves symbol relationships.
5. Independent corroboration: tests, build/typecheck/lint, history, docs/specs, or a second search shape.
6. Verdict: confirmed/likely/uncertain, with the falsifying check named.

### Confidence

- `confirmed`: two independent evidence sources agree, or one authoritative deterministic check proves the claim.
- `likely`: one source supports the claim, or the evidence is a reasoned approximation.
- `uncertain`: hypothesis, snippet, or incomplete proof path.

Search snippets, LLM judgment, and package popularity are leads. Exact reads, AST, LSP, PR/commit evidence, binary metadata, formal docs/specs, build/test/typecheck/lint, and reproducible command output can be proof.

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

Every finding should include claim, anchor, proof check, confidence, impact, and next action.
If a deterministic check was not run, say so and keep confidence below `confirmed`.
