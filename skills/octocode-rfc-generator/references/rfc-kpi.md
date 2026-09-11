# KPI.md template — success and verification

Load when defining acceptance and post-ship success. Why: bind RFC goals to testable behavior, measurable outcomes, guardrails, and a decision rule.
Reference `RFC.md` goals or standalone `PLAN.md` context; never restate them.

````markdown
# Success and Verification: {Title}

> Verifies {`RFC.md` §Goals | `PLAN.md` §Plan Context} and the implementation steps.

## User Stories
- As a {persona}, I want {capability}, so that {benefit}. → RFC goal #{n}

## Acceptance Criteria
```gherkin
Feature: {feature}
  Scenario: {name}
    Given {known state}
    When {action}
    Then {observable outcome}
```

## Definition of Done
- [ ] Acceptance scenarios pass
- [ ] V&V checks pass
- [ ] Documentation/resources updated
- [ ] Rollout completed per implementation plan

## Success Metrics
| Metric | Type | Baseline | Target | Window | Source |
|---|---|---|---|---|---|
| {primary outcome} | lagging | | | | |
| {driver} | leading | | | | |
| {must-not-regress} | guardrail | | | | |

## Decision Rule
- Success if {primary reaches target within window and guardrails hold}.
- Roll back or iterate if {measurable threshold/condition}. KPI.md owns the measurable rollback threshold; the plan owns the procedure.

## Traceability
| Primary requirement (§) | Implementation step(s) | Story | Acceptance check | Verification | Post-ship status |
|---|---|---|---|---|---|
````

Gate: every requirement maps to implementation step IDs, a pass/fail check, verification method, and current status; every step maps back to a requirement. Add a user story when it clarifies behavior.
Use outcome, leading, and guardrail metrics when they are decision-relevant. Mark omitted, untracked, or stale signals honestly rather than inventing targets.

Next: build dependency-ordered steps with `references/rfc-implementation.md`, then record provenance and validate per `references/workflow.md` § Validate and deliver.
