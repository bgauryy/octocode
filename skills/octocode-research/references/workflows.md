# Workflow selection

Load when the question spans more than one kind of work. This reference maps surface and task routes; the lobby owns the overall workflow.

## Choose a surface and a task
A surface tells you where evidence lives. A task tells you what evidence must establish. Combine the relevant pair and skip irrelevant or redundant stages when the answer is already known.

| Surface | When | Route |
|---|---|---|
| Local | checkout, artifact, or resolved dependency | `references/workflow-local.md` |
| External | remote repository, npm package, upstream history | `references/workflow-external.md` |
| Combined | source/version comparison or remote semantic proof | `references/workflow-combination.md` |

| Task | Needed outcome | Route |
|---|---|---|
| Lookup | one supported fact and exact anchor | direct read or the surface route |
| Bug investigation | trigger, violated contract, mechanism, alternate check | `references/workflow-debug.md` |
| Feature/enhancement | acceptance criterion or measured improvement | `references/workflow-change.md` |
| Refactor | preserved behavior, proven consumers, obsolete path removal | `references/workflow-refactor.md` |
| PR/local review | changed-code findings and verification-based recommendation | `references/workflow-pr-review.md` |

## Scale the evidence
- A known file/line or history identity can go directly to an exact read.
- Unfamiliar structure earns tree/files discovery; syntax questions earn AST; identity questions earn LSP; topology questions earn graph.
- Deletion, security, migration, and absence claims need broader corroboration than an ordinary lookup.
- Classify uncertain behavior with `references/problem-framing.md`; apply `references/code-research.md` for consequential code claims.
- Load `references/octocode.md` only when invocation or recovery is unclear. Do not preload every reference for a small question.

For Map/Validate/Investigate/Plan use `references/research-flow.md`; recurring uncertainty uses `references/loop-mode.md`. A contested long decision earns `references/long-research.md`; repository comparisons earn `references/github-landscape.md`; campaign coordination earns `references/researcher-mindset.md`.

## Continue with context
Debug can lead to an authorized change; upstream findings return to local verification. Keep the existing question, evidence, and user constraints while changing the next step.

Handoff only the useful state: `question | scope/ref/version | evidence/confidence/gaps | checks | next`.

Next: choose the relevant surface/task links above; return to `references/algorithm.md` when the next evidence source is uncertain.
