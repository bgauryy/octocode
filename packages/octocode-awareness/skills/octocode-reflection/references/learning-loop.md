# Learning Loop

Use this when a task produced a reusable lesson, user correction, skill-learning candidate, background-review finding, or failed/partial outcome.

## What To Keep

Record only information that changes a future decision:
- Verified root cause, decision, workaround, or gotcha.
- User correction that prevents repeating a bad behavior.
- Workflow that took multiple attempts and should become a skill or reference.
- Repeated failure with a stable `failure_signature`.
- Evidence that an existing memory is stale and should be superseded.

Skip routine status, raw logs, secrets, obvious edits, and facts already present in git/docs.

## Record

Use `memory_record` for durable reusable facts:

```typescript
memory_record({
  task_context: "why a future agent needs this",
  observation: "decision-changing lesson with evidence and verification",
  label: "DECISION", // pick the closest specific label — never default to OTHER
  importance: 7,
  references: ["file:/abs/path.ts:42", "test:yarn workspace pkg test"],
  supersedes: ["mem_old_id"], // when replacing stale knowledge
})
```

Labels drive retention and filtering.
`DECISION`, `ARCHITECTURE`, `SECURITY`, `GOTCHA`, and `OVERRIDE` decay with a 90-day half-life.
`EXPERIENCE` decays with a 14-day half-life, and everything else decays with a 30-day half-life.

A store full of `OTHER` defeats per-label decay and recall filtering.
Choose from `BUG`, `FEATURE`, `SUGGESTION`, `GOTCHA`, `IMPROVEMENT`, `DECISION`, `ARCHITECTURE`, `SECURITY`, `PERFORMANCE`, `TEST`, `BUILD`, `DOCS`, `CONFIG`, `WORKFLOW`, `REFACTOR`, `API`, `RELEASE`, or `INCIDENT`.
Reserve `OTHER` for lessons that truly fit nothing.
Use `OVERRIDE` only for critical invariants that contradict normal model defaults.

## Reflect

Use `memory_reflect` after non-trivial work, abandonment, repeated correction, or eval failure:

```typescript
memory_reflect({
  task: "split awareness and reflection skills",
  outcome: "worked", // worked | partial | failed
  lesson: "Awareness should stay live-coordination only; Reflection owns learning/cleanup.",
  fix_repo: "Update Pi docs to route post-task learning to octocode-reflection.",
  fix_harness: "Add staged approval checks before editing standing skill guidance.",
  failure_signature: "mechanism:skill-drift|cause:mixed-trigger-boundaries",
  judgment_note: "Validated with skill-lint and targeted package tests.",
})
```

`lesson` becomes durable memory. `fix_repo` opens a refinement. `fix_harness` creates a harness-tagged proposal for human review. Reflection records and proposes; it does not apply repo or skill edits by itself.

## Skill Learning

When a repeated workflow should become a skill:
1. Capture the source evidence and successful sequence.
2. Propose the skill name, trigger, workflow, resources, validation, and rollback.
3. Stage the proposal for approval using `references/staged-approval.md`.
4. After approval, create or update the skill with `octocode-skills`, then run skill lint and the package tests that prove it.

Prefer a small reference or script over bloating `SKILL.md`. Do not create a skill for one-off facts that memory recall already handles.

## Background Review

If a host supports background review, run this checklist after a turn: identify durable lessons, stale memories, skill-learning candidates, and harness proposals.
If the host has no background job, do it at finish time. In both cases, apply the same approval gate before mutating standing guidance.
