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
| Change separate parts concurrently | **[Parallel change lanes](composition.md#parallel-write-lanes):** partition paths → edit → integrate → check | Checks pass on the merged tree under one build owner. |
| Check a result independently | **Independent acceptance:** give checker contract and inputs → derive checks → inspect result | Consumer and edge checks pass against a stable acceptance condition. |
| Distinguish rival causes | **[Hypothesis checking](mental-models.md#issue-hunting-loop):** claim + rival → distinguishing probe → observation | The observation separates the causes, or the deciding gap is named. |
| Act on changing state | **Stateful procedure:** inspect prerequisites → act → confirm durable effect | The intended transition is observed; uncertain effects are reconciled before replay. |
| Diagnose waste or resource growth | **[Resource-flow audit](resource-flow.md):** map → measure → diagnose → repair → replay | A comparable measurement improves while correctness and required evidence remain intact. |

Roles are duties, not a worker count. Separate actors or concurrent work require authorization. For orchestrated execution, [composition](composition.md) owns assignments and handoffs; [verification](verification.md) owns the single ledger, acceptance and recovery. Domain skills retain their methods, permissions and output. A proposed selection needs no ledger.

## Evidence techniques

| Question | Read |
|---|---|
| Which assumption could fail, or which success metric could mislead? | [Mental models](mental-models.md#choose-the-lens); choose only lenses that change the next check. |
| What syntax or structural paths exist? | [AST workflow](ast-lsp-workflows.md#ast-workflow); matches are candidates, not runtime proof. |
| Which symbol is this, and who uses it? | [LSP workflow](ast-lsp-workflows.md#lsp-workflow); partial or empty results do not prove absence. |
| How should several things communicate? | [Interaction patterns](interaction-patterns.md); map agents, tools, services and artifacts to handoffs and state changes. |
| Which duties need separating? | [Role patterns](role-patterns.md); require a distinct action or check. |
| What remains after interruption? | [Compose and resume](composition.md#compose-and-resume); reconcile current state with existing evidence. |

## Task workflows

Choose the domain owner separately from the execution pattern. Use installed skills; these names do not imply availability.

| Task | Owner skill |
|---|---|
| Source, history or package evidence | `octocode-research` |
| Implementation, architecture or refactor | `octocode-engineer` |
| Diff review / cleanup / blunt critique | `octocode-pr-review` / `octocode-clean-agentic-code` / `octocode-roast`, respectively |
| Ideas or comparisons / design proposal | `octocode-brainstorming` / `octocode-rfc-generator`, respectively |
| Public-page extraction / live browser work | `octocode-scraping` / `octocode-chrome-devtools`, respectively |
| Documentation / skill maintenance | `octocode-documentation` / `octocode-skills`, respectively |
| Prompts, evaluation or cross-session coordination | `octocode-agentic` |

In the Octocode repository, `octocode-dev` owns builds, tests and contract generation. All roles may use available Octocode tools within their authority; see the [tool-use map](composition.md#octocode-tools-for-every-role).
