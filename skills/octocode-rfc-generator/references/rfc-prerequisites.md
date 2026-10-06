# RFC Prerequisites

Load when readiness evidence does not fit Motivation and Current State. Why: this file is the readiness record that has left the RFC.
Use `octocode-research` for repository, artifact, dependency, history, and migration evidence.

Every prerequisite needs an exact local/external citation or an open question with owner, next proof, and why work cannot start.

```markdown
# Prerequisites: {Title}

> Primary anchor: `{RFC.md | PLAN.md}` §{section}

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
| Blocker | Impact | Owner | Resolution before decision or Step 1 |
|---|---|---|---|

## Contracts and Migration Constraints
| Contract/data/API | Compatibility constraint | Rollback or guardrail |
|---|---|---|
```

Gate: do not decide or plan as though an unresolved blocker is satisfied. Record it in the primary artifact and close it before recommendation or before any step that depends on it; a blocker cannot be deferred as though executable. <!-- style-lint: ignore-line passive-voice -->
Cite only implementation-gating facts here.

Next: lock Goals and Non-Goals in `references/rfc-template.md` before `Recommendation: final`. Use `references/rfc-kpi.md` when measurement needs its own file; otherwise write the acceptance contract in `references/rfc-implementation.md` before the steps. When a readiness fact needs more proof, return to `references/research-playbook.md`.
