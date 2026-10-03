# Research algorithm

Load when the task class or first move is unclear. The lobby `First move` rule picks the first call.

Pick a surface and a task; skip irrelevant or redundant stages when a known anchor already answers.
- Surface: local → `workflow-local.md`; remote, docs, or local ↔ remote → `workflow-external.md`.
- Task: lookup → direct read; bug, root cause, or consequential claim → `code-research.md`; feature/enhancement/refactor → `workflow-change.md`; review → `workflow-pr-review.md`; multi-pass → `campaigns.md`.

| Class | Done when |
|---|---|
| bug | the lobby root-cause gate holds and a regression check exists |
| feature | consumers, acceptance tests, compatibility decision |
| enhancement | baseline, target, experiment, regression guard |
| unknown | one missing fact + cheapest check or focused question |

Failure and empty-result handling → `octocode.md`. Handoff only `question | scope/ref | evidence/confidence/gaps | checks | next`.

Next: load the route this page picked.
