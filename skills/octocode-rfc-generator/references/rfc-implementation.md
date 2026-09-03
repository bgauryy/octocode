# IMPLEMENTATION.md template — build document

Load when writing the live implementer plan. Why: close every RFC question with evidence, then order build, and verification by dependency.
Reference `RFC.md` anchors; never restate goals or scope. Success metrics stay in `KPI.md`.
```markdown
# Implementation: {Title}

> Decision: `RFC.md` §Summary / §Rationale

## Resolved Questions
| RFC open question (§) | Resolution or explicit deferral | Evidence | Confidence |
|---|---|---|---|

Decision-blocking questions are closed. Each remaining deferral names its impact, reason, owner, and revisit trigger.

## Approach
One or two lines linking the selected RFC recommendation.
## Steps
### Phase 1: {name}
- [ ] {dependency-ordered, shippable step} — `path:line` — RFC §{anchor}
### Phase 2: {name}
- [ ] {next step} — `path:line` — RFC §{anchor}

## Files, APIs, and Contracts
| Surface | Change | Blast-radius evidence | Compatibility |
|---|---|---|---|

## Risk Mitigations
| RFC risk/pre-mortem | Preventive action | Detection |
|---|---|---|

## Test and Verification Plan
| Type | Scope | Approach | Command |
|---|---|---|---|

Verification asks whether the build matches design; validation asks whether `KPI.md` moved.

## Rollout, Migration, and Rollback
- Sequence and observable proceed signal
- Flags/canary/percentage gate
- Rollback trigger mirrored in `KPI.md`
- Owner/approver

## Critical References
- {exact local or external anchor} — how it constrains the plan
```

Gate: every decision-blocking question is resolved with evidence; other questions are explicitly deferred. Include estimates only when requested and evidence-based; keep steps reversible where useful. Use `RESOURCES.md` when the source inventory needs its own file. <!-- style-lint: ignore-line passive-voice -->

Next: bind these steps to acceptance with `references/rfc-kpi.md`; record the source inventory with `references/rfc-resources.md`.
