# KPI.md Template — Success and Verification

Load when defining acceptance and post-ship success. Why: bind RFC goals to testable behavior, measurable outcomes, guardrails, and a decision rule.
Reference `RFC.md` goals; never restate them.

````markdown
# Success and Verification: {Title}

> Verifies `RFC.md` §Goals and the `IMPLEMENTATION.md` build.

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
- Roll back or iterate if {threshold/condition}; mirror the implementation rollback trigger.

## Traceability
| RFC requirement (§) | Story | Acceptance check | Verification | Post-ship status |
|---|---|---|---|---|
````

Gate: every RFC requirement has a story, pass/fail check, verification method, and current status.
Use at least one outcome, one leading signal, and one guardrail; mark untracked or stale rows pending rather than asserting success.
