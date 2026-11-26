Load when research spans several passes, sessions or actors, or unfinished or blocked work moves to someone else.

# Campaigns and handoff

## Campaigns

- Run each pass with the entry's [research loop](../SKILL.md#research-loop): one framed question, then the cheapest call that can change the answer. Respect the task's actual budget and prerequisites. Measure claims resolved, not calls.
- For work spanning sessions or actors, retain only what recovery needs: goal, source anchors, unresolved alternatives, next checks and emitted continuation calls.
- Direct research may pursue independent directions sequentially. When an orchestrator selects a multi-actor workflow, it assigns only directions with distinct observations, keeps one ledger and the merged verdict, and gives each actor an authorized Task contract: objective (purpose), constraints, output (`claim, evidence, verdict, confidence` per finding), and a Definition of Done. Actors pick the domain method and return evidence; they do not select more actors. Disagreement is evidence; the orchestrator re-checks each load-bearing anchor. When installed, `octocode-workflows` covers workflow composition.
- At decision boundaries and when stopping, report per [output](../output.md#response), naming verification and open gaps (unresolved alternatives, exhausted budgets, missing prerequisites).

## Handoff

Pick the protocol by what the receiver must do:

| Situation | Protocol | Slots |
|---|---|---|
| Blocked, or the next step needs someone else's evidence, decision, or action | SBAR | situation (question, scope/ref), background (checks run), assessment (evidence, confidence, gaps), request (next) |
| Unfinished work moves to another actor or session | Handoff packet | current state (question, scope/ref, checks run), evidence (anchors), blockers (gaps), next action, constraints, next-call ids (`next.*` calls copied unchanged) |

- Durable handoff (authorized, or [output](../output.md#saved-report) says save): the saved report; no evidence records.
- Export structured evidence only for a named machine consumer, using its actual schema and required source anchors.
