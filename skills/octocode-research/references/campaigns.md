# Campaigns

Load when one query is not enough: repeated Act → Observe → Learn loops, budgets, parallel workers, durable briefs, or repository landscapes.

## Loop

`frame one question → act (cheapest call that can change the answer) → observe status → learn → next call`
- Ledger: goal, anchors (paths, lines, ids, refs, `next.*`/`hints.*`; never invented), plausible competing explanations, tried shapes, cheapest disconfirming step.
- Stop when the question is answered or no available check can change the conclusion. State unresolved alternatives, exhausted budgets, or missing prerequisites.
- Summarize at decision boundaries or before context is lost: the current answer, deciding evidence, verification, and open gaps. Avoid a transcript of every call.

## Campaign control

- Open with corpus, question, mode, surfaces, budget, and stop test. Measure claims resolved, not calls made.
- Check reach first (`octocode.md`).
- Parallel workers fit independent directions or one claim down different lanes: tight brief, return `claim, evidence, verdict, confidence`; disagreement is evidence; re-check each worker's load-bearing anchor. Use authorized host worker tools; inspect the same directions sequentially when delegation is unavailable or unnecessary.

## Durable brief

Save a brief when requested or needed for an authorized handoff. Capture the question, scope, deciding evidence, gaps, and next checks. Keep internal evidence records separate from user-facing content. These JSON/JSONL examples serve a machine consumer; do not append them as metadata to a delivered brief:

```json
{"id":"ev1","type":"exact-file","source":"local","locator":"src/example.ts:42","quoteOrFact":"X calls Y","quality":"primary"}
{"id":"cl1","claim":"A supports X","status":"partial","confidence":"likely","supportingEvidenceIds":["ev1"],"counterEvidenceIds":[],"nextCheck":"read tests"}
```
Choose status and quality labels that help the consumer. Summarize the decision in chat and link saved evidence rather than dumping raw records.

## Repository landscape

For reuse decisions, discover plausible repositories and packages, then inspect enough candidates to distinguish fit and risk. Verify finalists through source, tests, releases, issues, and license as needed. Explain the comparison, integration path, and remaining gaps; popularity alone does not establish suitability.

Next: a surviving lead needing an edit → `workflow-change.md`; a finalist needing in-repo proof → `workflow-external.md`.
