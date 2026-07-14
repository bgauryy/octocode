# RFC Prerequisites

Load when an RFC changes existing code. Why: readiness facts, setup, owners, blockers, and baselines must be proven before implementation planning.
Write `PREREQUISITES.md` before `IMPLEMENTATION.md`; use `octocode-research` for repository, artifact, dependency, history, and migration evidence.

Every prerequisite needs an exact local/external citation or an open question with owner, next proof, and why work cannot start.

```markdown
# Prerequisites: {Title}

> RFC anchor: `RFC.md` §{section}

## Scope
Existing-code area and contracts affected.

## Required Current-State Evidence
| Requirement | Evidence | Confidence | Owner |
|---|---|---|---|

## Environment and Setup
| Need | How to verify | Source |
|---|---|---|

## Baseline Verification
| Check | Command or method | Expected baseline | Evidence |
|---|---|---|---|

## Blockers Before Implementation
| Blocker | Impact | Owner | Resolution before Step 1 |
|---|---|---|---|

## Contracts and Migration Constraints
| Contract/data/API | Compatibility constraint | Rollback or guardrail |
|---|---|---|
```

Gate: do not plan as though an unresolved blocker is satisfied. Put unresolved facts in `RFC.md` Open Questions and close or explicitly defer them in `IMPLEMENTATION.md`.
Keep long source inventories in `RESOURCES.md`; cite only implementation-gating facts here.
