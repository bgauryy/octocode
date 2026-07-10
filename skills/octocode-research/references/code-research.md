# Code Research

Load for code investigation, review, refactor, architecture, dead-code, artifact, or blast-radius work. Read `algorithm.md` first; load the matching `workflow-*.md` before this proof ladder.

## Route
| Need | First proof path |
|---|---|
| bug/behavior | reproduction → hypotheses → exact boundaries → AST/LSP/history |
| implementation | contract → blast radius → local pattern → patch → checks |
| refactor (preserve behavior) | skeleton → contracts → blast → big→small tasks → bulk move/rewire → verify (`workflow-refactor.md`) |
| PR/local review | changed region → symbols → consumers/tests → ranked findings |
| dead code/delete | candidate → LSP/AST/broad text/tests |
| architecture | entry points → dependencies/fan proxies → tradeoffs |
| binary/archive | `localBinaryInspect` inspect/list/strings → extract/unpack → local research |

## Workflow
1. State goal, scope, and expected output: research, review, plan, or patch.
2. Map structure/change/artifact before body reads; keep a likely and alternate hypothesis.
3. Read exact slices; use AST for shape and LSP for identity/reachability.
4. For edits, find a local pattern and patch only the evidence-supported boundary.
5. Run the declared test/build/typecheck/lint/smoke or deterministic read/search check.
6. On failure, keep the receipt, reread the failing path, patch only the cause, or report the exact block.
7. Report `confirmed`, `likely`, or `uncertain`; snippets and model judgment remain leads.

## Gates
Ask before public contracts, cross-layer/package changes, shared deletes/renames, many consumers, or product/architecture tradeoffs.

Review findings lead and include `file:line`, impact, evidence, confidence, and fix. Changes cannot claim success until verification runs; unavailable checks cap confidence below confirmed.

## Proof Ladder
`candidate search → exact read → AST shape → LSP identity/reachability → independent test/history/spec/check → verdict`

| Finding | Minimum corroboration |
|---|---|
| dead export / safe delete | broad text + LSP excluding declaration + AST/imports + tests/build |
| dependency cycle | imports both ways; mark SCC claims approximate without graph proof |
| security sink | sink shape + exact read + source/callers + guard/sanitizer check |
| test gap | important/changed symbol + no test refs + nearby test-tree read |
| coupling/god function | fan proxies + mixed responsibilities + callers/callees |
| performance | exact hot/independent path; benchmark only when runtime proof matters |

Dismiss a candidate when stronger proof contradicts it and state the reason briefly. Final output names claim, anchor, proof, confidence, impact, next action, and any deterministic check not run.
