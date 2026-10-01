# Campaigns

Load when one query is not enough: repeated Act → Observe → Learn loops, budgets, parallel workers, durable briefs, or repository landscapes.

## Loop
`frame one question → act (cheapest call that can change the answer) → observe status → learn → next call`
- `empty` ran and matched nothing: change one variable. `error` is a broken call: fix it, never read it as absence.
- Ledger: goal, anchors (paths, lines, ids, refs, `next.*`; never invented), two live hypotheses, tried shapes, cheapest disconfirming step.
- Stop when the question is answered and the alternate killed, no cheap step can change the conclusion, a budget or prerequisite blocks, or iterations stop changing state. A stall switches surface or shape: local ↔ GitHub ↔ packages ↔ history, text ↔ AST ↔ LSP ↔ graph, broad ↔ narrow.
- Checkpoint every 3-5 decisive steps. Output: Answer, Evidence, decisive Loop trace, Verification, Open gaps.

## Campaign control
- Open with corpus, question, mode, surfaces, budget, and stop test. Measure claims resolved, not calls made.
- Check reach first: `scheme` (enabled tools), `auth status` (GitHub), `lsp-server status <file>` (semantics).
- Parallel workers fit independent directions or one claim down different lanes: tight brief, return `claim, evidence, verdict, confidence`; disagreement is evidence; re-check each worker's load-bearing anchor. Mechanics → `octocode-subagent`.

## Durable brief
For consequential decisions, 3+ surfaces, conflicting claims, or requested saved research. Freeze question, surfaces, budget, stop gates, non-goals; save `research_campaign.json`, `evidence.jsonl`, `claims.jsonl` only when artifacts are approved.

```json
{"id":"ev1","type":"exact-file","source":"local","locator":"src/example.ts:42","quoteOrFact":"X calls Y","quality":"primary"}
{"id":"cl1","claim":"A supports X","status":"partial","confidence":"likely","supportingEvidenceIds":["ev1"],"counterEvidenceIds":[],"nextCheck":"read tests"}
```
Quality: primary/secondary/weak/counter. Status: supported/partial/contradicted/unverified/dropped. Output `TL;DR · claims · evidence by surface · verdict · risks · next`; never dump raw JSONL.

## Repository landscape
For reuse decisions or ecosystem comparison: frame literal/alias/package terms → discover repos and packages → rank cheaply → deep-read the top 3-8 (tree, README, source/tests, issues/PRs, releases, license) → score fit, evidence, activity, reuse, risk (stars/downloads are tiebreakers) → clusters, ranked table, finalist proof, integration path, risks.

Next: a surviving lead needing an edit → `workflow-change.md`; a finalist needing in-repo proof → `workflow-external.md`.
