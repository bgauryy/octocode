# Brainstorming phases

Load for deep, delegated or resumable research. The ordinary loop is the steps in `SKILL.md`; this page adds orchestration, checkpoints and recovery to steps 2–5, [gather to decide](phases-gather-decide.md) to steps 6–9, and [workers](workers.md) says who sees what. `<run>` is the run folder `SKILL.md` names.

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
| Requested steering, an explicit user stop, or a missing scope, spend or authority decision | Pause the work that depends on it; continue the other authorized work. | Respect the decision boundary. |
| Resume | Continue the unfinished phase; rerun `research-tree.mjs --plan <run>/plan.jsonl --verified <run>/ledgers/<worker>.verified.jsonl` (one `--verified` per worker) to see what is open; reopen evidence only if identity, applicability or claim changed. | Avoid repeated research. |
| Stopped | Deliver verified interim and what the next phase adds. | Preserve a usable handoff. |

## Plan and critique

Steps 4–5. The plan is the contract between you and the workers: one [plan row](research-questions.md#research-plan) per claim, its rival, its refuting observation, and queries in both directions on the surfaces that hold the evidence ([web vendors](research.md#vendors), [GitHub](research-github.md#github)). `research-tree.mjs --plan` refuses a claim without a rival, a refuting observation or a query in each direction, and groups the claims per worker. Group the rows so each gatherer owns a few claims with shared sources; a sub-question whose claims need different surfaces can split across workers. [Merge or drop](research-questions.md#research-question) questions first.

Critique the plan before dispatch, then fix it and rerun the check; a weak rival or a disconfirming query that cannot find counterevidence wastes the whole round:

- Each rival is the strongest one, not a straw man.
- Each disconfirming query could really find failures, limits, critiques or the rival's best case.
- The evidence sought is likelier under the claim than under its rival ([hypotheses](critical-thinking.md#hypotheses-before-targeted-search)).
- The user's own assumptions are claims too.
- A costly or irreversible choice adds its [premortem](judging.md#checks-for-critics-and-the-judge) causes as claims.
- A claim nothing could refute is a framing: sharpen or drop it.

## Scripts

Run `node <skill-folder>/scripts/<name>.mjs --help` for options and `node --test '<skill-folder>/scripts/tests/*.test.mjs'` for tests. Scripts share the [network guard](../scripts/lib/net-guard.mjs) and [JSON errors](../scripts/lib/fail.mjs).
