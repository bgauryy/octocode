# RFC Prerequisites

Load this when an RFC changes existing code. Write `PREREQUISITES.md` before `IMPLEMENTATION.md` or any step-by-step implementation plan.

Purpose: separate facts that must be true before building from the build steps. Prerequisites are evidence, setup, owners, blockers, and baseline checks; they are not Phase 0 code tasks unless they change code.

## Research Before Writing

Use `octocode-research` for repository facts, packaged artifacts, generated binaries, dependencies, prior art, history, and migration examples.
If the skill is not installed, use https://github.com/bgauryy/octocode/tree/main/skills/octocode-research or install it with `npx octocode skill --name octocode-research`.

Every prerequisite needs one of:

- Local citation: `path:line`.
- External citation: GitHub file/line, PR, commit, package, or docs link.
- Explicit open question: owner, next proof, and why implementation cannot start until it is answered.

## Output Template

```markdown
# PREREQUISITES

## Scope

Existing-code area affected:
RFC anchor:

## Required Current-State Evidence

| Requirement | Evidence | Confidence | Owner |
|---|---|---|---|

## Environment And Setup

| Need | How to verify | Source |
|---|---|---|

## Baseline Verification

| Check | Command or method | Expected baseline | Evidence |
|---|---|---|---|

## Blockers Before Implementation

| Blocker | Impact | Owner | Resolution needed before Step 1 |
|---|---|---|---|

## Contracts And Migration Constraints

| Contract/data/API | Compatibility constraint | Rollback or guardrail |
|---|---|---|
```

## Gate

Do not write `IMPLEMENTATION.md` as if prerequisites are satisfied when any blocker is unresolved. Put unresolved prerequisite facts into `RFC.md` Open Questions and close or defer them in `IMPLEMENTATION.md` with `octocode-research` evidence.
