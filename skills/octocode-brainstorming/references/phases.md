# Brainstorming phases

Load for deep, delegated or resumable research. The ordinary loop is the steps in `SKILL.md`; this page adds orchestration, checkpoints and recovery to them. `<run>` is the run folder `SKILL.md` names.

```text
understand → diverge → plan tree → critique plan → small workers: leads → findings → combine + deepen → check → decide
   (2)         (3)        (4)            (5)                   (6)                         (7)            (8)      (9)
                            ↑────────────── a named gap that could change the recommendation ──────────────────────┘
```

## Understand and diverge

Steps 2–3. Reuse supplied or known prior work when applicable; use [report discovery](../output.md#saved-report) only when locating a saved result.

- Each pass takes one lens that can change the decision, under its own persona or constraint, worked step by step; a pass never reads another pass's options, because a model's first ideas anchor the rest ([evidence](method-evidence.md#independent-passes)).
- With workers, give each pass a [diverger brief](briefs.md#diverger-brief) in a fresh context, on a different model when one is offered, writing `<run>/passes/<pass>.jsonl`; without workers write each pass before reading the others.
- Then one exploratory search for options nobody listed: `node <skill-folder>/scripts/search.mjs --query "<the decision in plain words>" --query "alternatives to <the user's current plan>"`; its `related` searches and questions are leads.
- Pool with `node <skill-folder>/scripts/pool-options.mjs --options <run>/passes/<pass-a>.jsonl --options <run>/passes/<pass-b>.jsonl` (or one `<run>/options.jsonl`). Give each true duplicate among its `candidates` a `sameAs`, rerun, drop the infeasible, and keep each option's origin (user, yours, found). Stop adding passes when `saturated` is true or no remaining lens can change the decision.

| Trigger | Action | Why |
|---|---|---|
| Work may be interrupted or handed off | Keep a [checkpoint](../output.md#checkpoints) with the plan, deciding evidence and the next action. | Recovery, not routine reporting. |
| Requested steering or missing scope, spend or authority decision | Pause dependent work. | Respect the decision boundary. |
| Resume | Continue the unfinished phase; rerun `research-tree.mjs --plan <run>/plan.jsonl --verified <run>/verified.jsonl` to see what is open; reopen evidence only if identity, applicability or claim changed. | Avoid repeated research. |
| Stopped | Deliver verified interim and what the next phase adds. | Preserve a usable handoff. |

## Plan and critique

Steps 4–5. The plan is the contract between you and the workers: one [plan row](briefs.md#research-plan) per claim, its rival, its refuting observation, and queries in both directions on the surfaces that hold the evidence ([web vendors](research.md#vendors), [GitHub](research.md#github)). `research-tree.mjs --plan` refuses a claim without a rival, a refuting observation or a query in each direction, and groups the claims per worker. Group the rows so each gatherer owns a few claims with shared sources; a sub-question whose claims need different surfaces can split across workers. Drop a question whose answer would leave the recommendation unchanged.

Critique the plan before dispatch, then fix it and rerun the check; a weak rival or a disconfirming query that cannot find counterevidence wastes the whole round:

- Each rival is the strongest one, not a straw man.
- Each disconfirming query could really find failures, limits, critiques or the rival's best case.
- The evidence sought is likelier under the claim than under its rival ([hypotheses](critical-thinking.md#hypotheses-before-targeted-search)).
- The user's own assumptions are claims too.
- A costly or irreversible choice adds its [premortem](critical-thinking.md#checks-for-critics-and-the-judge) causes as claims.
- A claim nothing could refute is a framing: sharpen or drop it.

## Gather: leads, then findings

Step 6. Delegate only independent groups whose answers can change the decision, each with the [gatherer brief](briefs.md#gatherer-brief), its plan rows and its own ledger path `<run>/ledgers/<worker>.jsonl`. Small scopes and a fast model suit gathering; you keep judgment. Keep keys out of briefs. A gatherer reads each deciding page with `node <skill-folder>/scripts/read-page.mjs --url <url> --grep "<phrase>"`, which prints the text the verifier sees, copies the quote with `decides` set to the claim id, and runs `verify-claims.mjs` on its own ledger. The orchestrator owns deciding-file downloads and their provenance: [verification and assets](verification.md#when-a-deciding-source-is-a-file).

## Combine and deepen

Step 7. Rerun [verify-claims.mjs](../scripts/verify-claims.mjs) on every worker ledger before counting it: `node <skill-folder>/scripts/verify-claims.mjs --ledger <run>/ledgers/<a>.jsonl --ledger <run>/ledgers/<b>.jsonl --out <run>/verified.jsonl` counts each ledger and the total (`--reuse <run>/verified.jsonl` on reruns, `--since` for the window; a page that blocks the script is read through an Exa or Tavily copy, named in `via`). A worker's "verified" is a claim, not a result. Join the result to the plan with `research-tree.mjs --verified`; its `deepen` list is the next round, briefed with what each claim still lacks, crossing web and GitHub ([combine and deepen](research.md#combine-and-deepen)). `unplanned` rows belong to no claim: map them to one or drop them. Stop deepening when a round changes no claim's state or status.

## Check

Step 8. Apply the [validated-source checks](verification.md#validated-sources) to each deciding row: the quote says what the claim says, the date, who is behind it, primary or traced to its study. Run the [contradiction pass](critical-thinking.md#contradict-and-prove) on every deciding claim, judging it against its supporting and disconfirming rows. Mark the status on its ledger rows (`decides`, `direction`, `claimStatus`) and rerun the verifier with `--reuse`: its `decidingClaims` gives each claim's support, refutation, sources and conflicting statuses. Dispatch an [independent critic](briefs.md#critic-brief) when its check can change the decision; it may cover several directions. Debate only when the [settling-test gate](critical-thinking.md#debate-when-and-how) holds. Verify new evidence before it counts.

## Decide

Step 9. Build the option table from `decidingClaims` and compare options on the same criteria, one at a time. Apply the [bias checks](critical-thinking.md#judging-without-bias) and [claim checks](critical-thinking.md#checks-for-critics-and-the-judge) to the verified quotes. Close each claim with its status, recompose the sub-answers (check that each part's conditions hold together) and say what changed from the expected answer. Give [calibrated confidence](critical-thinking.md#calibrated-confidence), the smallest next action and the check most likely to change the recommendation. Stop when further research cannot change it.

## Workers

You are the orchestrator: you frame, plan, pool, judge and decide; workers only generate, gather or check. Each worker gets a brief from [briefs](briefs.md) and sees nothing else of the conversation.

| Role | Dispatch when | Sees | Writes | You then |
|---|---|---|---|---|
| Diverger | Step 3, one per pass that can change the decision | The frame and its own lens, persona or constraint; never another pass | `<run>/passes/<pass>.jsonl` | Pool with `pool-options.mjs`, judge `candidates`, keep origins |
| Gatherer | Step 6, one per worker group of the plan; again in step 7 for `deepen` claims | Its plan rows | `<run>/ledgers/<worker>.jsonl` | Rerun `verify-claims.mjs` on the ledger, join it with `research-tree.mjs` and apply the [validated-source checks](verification.md#validated-sources) |
| Critic | Step 8, when its check can change the decision | The conclusion and verified ledger, before others' verdicts | Issues with repairs | Verify new evidence before it counts |
| Debaters, judge | Only when the [debate gate](critical-thinking.md#debate-when-and-how) holds | The resolution and verified ledger | Cases; a verdict | Recheck a close verdict in reverse order |

- Dispatch independent workers in one batch; a worker never reads another's output before committing its own.
- A worker's rating, ranking, confidence or "verified" is a claim, not a result: count only what you re-verified ([evidence](method-evidence.md#self-evaluation)).
- Use a different model for a diverger or critic when the host offers one: pooling across vendors widens options ([evidence](method-evidence.md#independent-passes)); for critics the gain is untested ([evidence](method-evidence.md#model-heterogeneity)).
- Workers never spawn actors, edit another worker's file or see keys. A worker that returns nothing usable gets one re-brief naming the gap; after that its claims are named gaps.

## Scripts

Run `node <skill-folder>/scripts/<name>.mjs --help` for options and `node --test '<skill-folder>/scripts/tests/*.test.mjs'` for tests. Scripts share the [network guard](../scripts/lib/net-guard.mjs) and [JSON errors](../scripts/lib/fail.mjs).
