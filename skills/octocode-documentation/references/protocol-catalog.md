Load only when a task, finding, handoff or decision needs a shared message format.
<!-- Source: skills/common; installed copies are generated. -->

## Catalog

Choose the smallest format the receiver needs. Use its task-specific slots; do not add protocol names, ratings, fields or status tables to an ordinary answer merely to demonstrate compliance. Keep evidence beside important claims and mark unverified slots unknown.

### Describe and decide

| Need | Protocol and useful slots |
|---|---|
| Issue or task | **5W2H:** what, where, trigger, affected consumer, cause, reproduction/repair, impact. |
| Incomplete evidence | **Facts, assumptions, unknowns:** distinguish what was checked from what was inferred. |
| Consequential finding | **Hypothesis test:** claim, strongest rival, distinguishing check, result. |
| Observable acceptance | **Given–When–Then:** state, action, expected result. |
| Architecture decision | **ADR:** context, decision, status, consequences. |
| Decision outside your authority | **Decision request:** options, trade-offs, recommendation; no default approval. |
| Review feedback | **Conventional Comments:** issue/suggestion/question, blocking or not, location, reason. Substantive findings also need evidence and impact. |

### Delegate and coordinate

| Need | Protocol and useful slots |
|---|---|
| Delegation | **Task contract:** objective, inputs, authorized scope/constraints, output and observable Definition of Done. |
| Open-ended work | **Stop condition:** verified acceptance or a named blocker/boundary; use the owner's resource limits. |
| Shared work | Name each task's owner, dependencies and handoff. Add a context manifest or budget only when the work needs it. |
| Consequential ambiguous instruction | **Check-back:** resolve the uncertain objective/effect before dependent work; skip routine restatement. |

### Report and hand off

| Need | Protocol and useful slots |
|---|---|
| Result | **BLUF / claim–evidence:** conclusion, deciding evidence, uncertainty and next action when needed. |
| Unfinished transfer | **Handoff packet:** current state, evidence, blocker, next action, constraints and executable continuation if needed. |
| Another owner's action | **SBAR:** situation, background, assessment, recommendation. |
| Progress | **Delta update:** meaningful change, blocker and next action. |
| Recovery after interruption | **Checkpoint–resume:** goal, completed/in-progress work, next action, unresolved effects and relevant files. |

### Loops and failures

Retry only when diagnosis, input or observed state justifies it, within the owning protocol's limits. Reconcile uncertain side effects before replay. Use a baseline and acceptance check to decide whether to keep a workflow change. A retrospective records intended outcome, observation, cause of the difference and improvement.

### Writing

Use plain imperatives. Reserve MUST/NEVER for safety, authority and hard contracts. For human documentation, choose tutorial, how-to, reference or explanation as needed. Prefer actual actions to acronyms or invented loop names. Omit raw logs, private deliberation and unrelated history from handoffs.
