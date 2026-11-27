Load when choosing how work should proceed.

# Workflow catalog

Choose by the dependency or proof the task needs. Use one primary workflow; add a technique only when it answers a distinct question. Ordinary tasks go directly to their domain skill.

## Execution patterns

| Need | Workflow and path | Done when |
|---|---|---|
| One scoped result | **Bounded execution:** input → domain action → check | The domain acceptance check passes. |
| Cover a whole system | **[Single-actor BFS/DFS audit](single-actor-traversal.md):** inventory broadly → trace each path deeply → reconcile coverage | Every discovered path is accounted for; unchecked scope is explicit. |
| Change running behavior | **[Runtime surgery](runtime-surgery.md):** reproduce → prove cause → change/build → activate → replay | The intended loaded artifact passes the same acceptance check. |
| Investigate separable questions | **Independent evidence lanes:** assign questions → gather primary evidence → merge | Deciding sources are rechecked; conflicts and gaps are resolved or reported. |
| Change separate parts concurrently | **[Parallel write lanes](parallel-write-lanes.md):** partition paths → edit → integrate → check | Checks pass on the merged tree under one build owner. |
| Check a result independently | **Independent acceptance:** give checker contract and inputs → derive checks → inspect result | Consumer and edge checks pass against a stable acceptance condition. |
| Distinguish rival causes | **[Hypothesis checking](mental-models.md#issue-hunting-loop):** claim + rival → distinguishing check → observation | The observation separates the causes, or the deciding gap is named. |
| Act on changing state | **Stateful procedure:** inspect prerequisites → act → confirm durable effect | The intended transition is observed; uncertain effects are reconciled before replay. |
| Diagnose waste or resource growth | **[Resource-flow audit](resource-flow.md):** map → measure → diagnose → repair → replay | A comparable measurement improves while correctness and required evidence remain intact. |

A workflow name sets no worker count: split duties across actors only under the [orchestrator and actor contract](composition.md#orchestrator-and-actor-contract). A proposed selection needs no ledger; an executed one keeps the [coverage ledger](verification.md#keep-one-compact-coverage-ledger).

## Task workflows

Choose the domain owner separately from the execution pattern. Use installed skills; these names do not imply availability.

| Task | Owner skill |
|---|---|
| Source, history or package evidence | `octocode-research` |
| Implementation, architecture or refactor | `octocode-engineer` |
| New app or project scaffold | `octocode-generate` |
| Diff review / cleanup / blunt critique | `octocode-pr-review` / `octocode-clean-agentic-code` / `octocode-roast`, respectively |
| Ideas or comparisons / design proposal | `octocode-brainstorming` / `octocode-rfc-generator`, respectively |
| Public-page extraction / live browser work | `octocode-scraping` / `octocode-chrome-devtools`, respectively |
| Documentation / skill maintenance | `octocode-documentation` / `octocode-skills`, respectively |
| Prompts, evaluation, one agent's harness or agent messaging setup | `octocode-agentic` |

In the Octocode repository, `octocode-dev` owns builds, tests and contract generation.
