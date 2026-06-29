# Code Research

Read this when `octocode-research` is handling code investigation, implementation, review, refactor, architecture, dead-code/safe-delete, PR/local diff, binary/archive, or blast-radius work.

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
8. Report confidence as `confirmed`, `likely`, or `uncertain`; never upgrade a snippet to proof without a check.

## Review And Change Gates

Ask before continuing when the next step changes a public contract, crosses packages/layers, deletes/renames shared exports, affects many consumers, requires a product/architecture tradeoff, or conflicts with current evidence.

For a review, lead with findings ordered by severity. Each finding needs `file:line`, impact, evidence, and a fix path. If there are no findings, say that and name residual test/risk gaps.

For a code change, do not claim success until the verification command actually ran. If verification is impossible, say why and keep confidence below `confirmed`.
