# KPI.md Template — Success & Verification

`KPI.md` is the **verification document**: it outlives the ship date and answers "how do we know the RFC *and* its implementation actually worked?"
KPI.md is more than dashboards: it holds the **acceptance criteria to check**, the **measurable success signals**, and the **traceability matrix** that binds all three files and detects drift.

This file **references `RFC.md` goals — it does not restate them.** Each goal in `RFC.md` §Goals maps to at least one user story and one measurable signal here.

```markdown
# Success & Verification: {Title}

> Verifies `RFC.md` §Goals and the `IMPLEMENTATION.md` build. Goals are referenced, not restated.

## User Stories to Check

- **As a** {persona}, **I want** {capability}, **so that** {benefit}.  (→ `RFC.md` §Goals #{n})

## Acceptance Criteria (Gherkin)

Testable pass/fail conditions — these double as the post-ship verification script.

\```gherkin
Feature: {feature}
  Scenario: {name}
    Given {known state}
    When {action}
    Then {observable expected outcome}
\```

## Definition of Done

Whole-feature quality bar (applies on top of per-story criteria):
- [ ] Acceptance scenarios pass
- [ ] Tests added and green (see `IMPLEMENTATION.md` V&V)
- [ ] Docs updated
- [ ] Rolled out per `IMPLEMENTATION.md`

## Success Metrics

Never a single metric — pair a system/telemetry signal with a perceptual/qualitative one, and name what must *not* regress.

| Metric | Type | Baseline | Target | Window | Source |
|--------|------|----------|--------|--------|--------|
| {primary outcome} | lagging (did it work) | {value} | {value} | {e.g. 4 weeks} | {measurement} |
| {driver} | leading (predicts outcome) | {value} | {value} | {window} | {measurement} |
| {guardrail} | counter-metric (must not regress) | {value} | ≥ {floor} | {window} | {measurement} |

_Delivery health (optional, for shipped code):_ note any DORA signal (change-fail rate, lead time, recovery time) that must not degrade.

## Decision Rule

- **Success if** {primary metric ≥ target within window}.
- **Roll back / iterate if** {metric < X after N period} — mirror the `IMPLEMENTATION.md` rollback trigger.

## Traceability Matrix

The anti-drift, anti-rot spine. Every `RFC.md` requirement must appear here, mapped forward to a check and a post-ship status.

| RFC requirement (§) | User story | Acceptance criteria | Verification method | Post-ship status |
|---|---|---|---|---|
| {req} | {story} | {Gherkin scenario} | {test / metric / manual} | pending / pass / fail |
```

> Naming note: the file is `KPI.md` but its H1 is "Success & Verification".
> Its job spans acceptance criteria, one-time post-ship verification, and ongoing metrics — not dashboards alone.
> A stale KPI file asserting untracked targets is worse than none. Keep the traceability matrix current or mark rows `pending`.
