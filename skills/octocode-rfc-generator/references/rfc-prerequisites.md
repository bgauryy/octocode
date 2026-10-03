# RFC Prerequisites

Load when an RFC or plan changes existing code. Prove readiness facts, setup, owners, blockers, and baselines before the decision or plan.
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

Next: once no blocker has to be assumed satisfied, decide or confirm the direction. Define separate acceptance with `references/rfc-kpi.md` when warranted; otherwise define it inline before steps through `references/rfc-implementation.md`. When a readiness fact needs more proof return to `references/research-playbook.md`. <!-- style-lint: ignore-line passive-voice -->
