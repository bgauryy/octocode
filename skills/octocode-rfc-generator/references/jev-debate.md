# Two agents, one bounded disagreement

Load before spawning workers for the explicit protocol in `references/jev-review.md`. The protocol supplies frozen evidence. It does not establish a quality or token benefit.

## Dispatch packet

The host owns requester communication, workspace writes, evidence checks, and final disposition. Workers are read-only consultants: no nested agents, no external actions. Give both workers:

- Goal, question IDs and types, frozen RFC revision, exact raw evidence with scope and date, constraints, and owner criteria.
- Include/exclude boundaries, available tools, authority, and the shared deadline. Default: opening 600 words, rebuttal 300 words, at most four targeted evidence reads per worker. Report truncation or an exhausted budget as partial.
- One role. The **proposal advocate** develops the strongest evidence-compatible answer and admits its failure conditions. The **adversarial reviewer** seeks missing blockers, counterexamples, and viable alternatives. Neither defends a false claim or invents disagreement.
- Required return: `status` (complete/partial/blocked), per-question proposed answer, source anchors, assumptions, strongest counterargument, falsifying or deciding check, and gaps. Concise public arguments only, never private chain-of-thought.

Use fresh-context workers with no inherited preferred conclusion when the host supports it. Keep raw inputs identical except role. If isolation is unavailable, record the limitation; two instances of one model are not independent replicates.

## Exchange and judge

1. **Opening barrier.** Collect both openings before either sees the other. If a worker fails, preserve partial work and label the debate incomplete. Never submit a one-sided packet as a completed debate.
2. **Shared evidence.** Inspect sources the workers cite, then give both workers the same additions. Unsupported citations stay claims. Source changes invalidate affected statements.
3. **Rebuttal barrier.** Send each worker the other's public opening. If the proposal emerged during openings, freeze its exact text, revision, questions, and criteria first and give both workers that contract. Ask for concessions, the remaining precise disagreement, and the single strongest deciding check. Stop after one rebuttal each. If they converge, or inspected evidence or a direct check settles the question, record the outcome and omit `clasify`. A later subject change needs affected worker review or an incomplete label; do not relabel old arguments as review of a new proposal.
4. **Judge packet.** Keep both openings and rebuttals, or faithful bounded summaries with original receipt links. Use anonymous A/B labels, identical evidence IDs, missing evidence, dissent, decision criteria, and applicable question IDs. Never instruct the judge to favor one agent or name the host's desired winner. Record a content hash or revision. Run `node scripts/validate-debate.mjs request.json worker-packet.json` from this skill folder (resolve paths explicitly from elsewhere). On a failed check, stop, repair packet construction, and keep the original receipt.
5. **Classification request.** Map the unresolved object through `references/jev-api.md`. Jev assesses the claim or proposal from the supplied evidence; it need not pick a winning speaker, and both arguments can be wrong. Record the request, every page-local typed result, requested model, provider-resolved model, host action, usage, and elapsed time when available. Never collapse pages into an unstated whole-resource answer.
6. **Host check.** Inspect the original deciding evidence or run the selected check, then apply `references/rfc-completeness.md`. A low-confidence or insufficient result stays open; a high-confidence result without proof also stays open.

A follow-up round needs a changed proposal, new deciding evidence, or changed authorized criteria, with its effect recorded before dispatch. A new label or paraphrase is not a new crossroad. Stop at the earliest of deadline, call cap, or no progress.

## Frozen packet shape

- `review: {id, rfcRevision, goal: "one-line decision goal", questions: [{id:"Q1", type, instructions, criteria?}], criteria: ["decision criterion"], subject: {kind: "proposal", text: "exact proposed action"}}`. Use `kind: "claim"` for a bounded factual or causal claim.
- `evidence`: an object keyed `E1`, `E2`, …; each value has `source` and `observation`. Keep optional fields. Never consolidate or renumber after dispatch.
- `admission: {workersDisagree:true, remainingDisagreement, evidenceDoesNotSettleBecause, directCheckUnavailableBecause, currentAction, ifJudgeSupports, ifJudgeRejects, workerPositions:{A,B}, willChangeAction:true, directCheck:{available:false}, evidenceFresh:true, clasifyCallsAtCrossroad:0}`. Positions and result-dependent actions must differ. Admission is host policy, never provider evidence.
- `context.value.arguments.A` and `.B`: objects with non-empty `opening` and `rebuttal` strings.

This preflight accepts exactly one source-free resource, so content the workers did not inspect cannot enter the judgment. Use separate SemanticQueries when evidence snapshots differ; batch with root `queries[]` only for independent complete queries. For a subset or changed subject, freeze a new scoped contract for both workers; never select it silently after debate. The projection into the SemanticQuery is owned by `references/jev-api.md`.

The preflight checks review identity, the judged subject, both argument rounds, and exact evidence preservation. It cannot prove workers consumed the contract, verify source truth, detect every misleading summary, or establish owner authority or semantic closure. Keep raw dispatches and worker messages so the host can audit coverage. When changing the helper, run `node scripts/validate-debate.mjs --self-test`.

## Receipt and contribution

Store the input revision, both worker outputs and rebuttals, exact request/response, requested and provider-resolved models, availability/transport outcome, deciding check, and ledger changes under the review directory. Keep raw provider results beside the host interpretation; record the host's next action separately. No credentials or hidden reasoning in receipts.

After verification, record `intended action before | judgment field | action after | checked outcome | discovery origin | incremental contribution | cost`. Classify contribution as changed action, prioritized an existing concern, confirmation only, no demonstrated help, or harmful. This is descriptive attribution, not proof of accuracy benefit.

Next: cost receipt and benefit comparison → `references/review-cost.md`.
