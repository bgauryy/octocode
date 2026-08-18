# Long Research

Rare path: load for durable decision briefs, claim traceability, or frozen campaigns. Use for consequential decisions, 3+ evidence surfaces, conflicting claims, or requested saved research.
Keep quick work in chat.

## Campaign Spec
Freeze only fields that guide the decision; save `research_campaign.json` only after artifact approval.

| Field | Content |
|---|---|
| question/mode | exact decision; map/validate/investigate/plan/review/change/loop |
| surfaces | active/skipped sources with reasons |
| budget | max iterations/deep dives/time boundary |
| stopGates | proof, budget, conflict, or user-choice stops |
| nonGoals | explicit exclusions |

Update the spec only when scope changes. Gate budget expansion or public-contract changes.

## Ledgers
`evidence.jsonl` rows are small proof facts:

```json
{"id":"ev1","type":"exact-file","source":"local","locator":"src/foo.ts:42","quoteOrFact":"X calls Y","quality":"primary","retrievedAt":"2026-06-28"}
```

Types: exact-file/LSP/AST/PR/commit/package/paper/official-doc/web/benchmark/test. Quality: primary/secondary/weak/counter. Promote snippets only after exact source, semantic/structural proof, metadata, history, or tests.

`claims.jsonl` rows are atomic support states:

```json
{"id":"cl1","claim":"A supports X","status":"partial","confidence":"likely","supportingEvidenceIds":["ev1"],"counterEvidenceIds":[],"nextCheck":"read tests"}
```

Status: supported/partial/contradicted/unverified/dropped; confidence: confirmed/likely/uncertain. Drop unsupported plausibility; retain decision-relevant contradictions.

## Workflow And Output
State spec → seed claims/alternates/disconfirmers → run `references/research-flow.md` → promote evidence → update claims → Advocate/Critic → brief from supported/partial claims → optionally capture one durable lesson.

Vendor web adapters may enrich web/paper evidence but never replace local/GitHub/npm/history/AST/LSP proof; on adapter failure continue natively and mark degraded.

Output: `TL;DR | scope | claim summary | evidence by surface | rebuttal survivors | verdict | risks/gaps | next | approved artifacts`. Summarize ledgers in chat; never dump raw JSONL.

Next: execute each surface through `references/research-flow.md`; when candidate repos need ranking load `references/github-landscape.md`; when a claim keeps flipping between iterations load `references/loop-mode.md`; when the brief authorizes an edit hand off to `references/workflow-change.md`.

