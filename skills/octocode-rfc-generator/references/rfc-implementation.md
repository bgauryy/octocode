# IMPLEMENTATION.md Template — Build Document

`IMPLEMENTATION.md` is the **build document**: implementer-facing and **live** during the work. Its distinctive job is to **close every open question left in `RFC.md` using `octocode-research` evidence**, then lay out a dependency-ordered, verifiable plan.

This file **references `RFC.md` section anchors — it does not restate goals or scope.** Steps trace back to RFC design decisions; success metrics live in `KPI.md`.

```markdown
# Implementation: {Title}

> Decision: see `RFC.md` §Summary / §Rationale. This file does not restate goals — it references them.

## Resolved Questions (were open in RFC.md)

Every `RFC.md` open question, closed with evidence or explicitly deferred. Nothing here may stay `uncertain`.

| Open question (RFC §) | Resolution | Evidence (`octocode-research`) | Confidence |
|---|---|---|---|
| {question} | {answer} | [`src/x.ts:42`](https://github.com/owner/repo/blob/main/src/x.ts#L42) — {why it settles the question} | confirmed / likely |
| {question} | Deferred — {reason}; revisit when {trigger} | — | n/a |

## Approach

{Which recommendation from `RFC.md` §Rationale is being implemented, in one or two lines.}

## Steps (dependency-ordered)

Ordered so each step is shippable and reversible where possible; foundational/riskiest pieces land where they can be validated cheaply. No time estimates.

### Phase 1: {name}
- [ ] Step — `path/to/file:line` (traces to `RFC.md` §Reference-Level)
- [ ] Step — `path/to/file`

### Phase 2: {name}
- [ ] Step — `path/to/file`

## Files / APIs / Contracts Touched

- `path/to/file:line` — what changes and why; cite the blast radius returned by `octocode-research`.

## Risk Mitigations

{Concrete action per risk — traces to `RFC.md` §Drawbacks / §Pre-mortem.}

## Test and Verification Plan (V&V)

Verification = did we build it to the design? Validation = did it move the KPIs (see `KPI.md`)?

| Type | Scope | Approach | Command |
|------|-------|----------|---------|
| Unit | {components} | {approach} | `{command}` |
| Integration | {flows} | {approach} | `{command}` |
| Performance | {metrics} | {approach} | `{command}` |

## Rollout / Migration / Rollback

- **Sequencing:** ordered, each step with its blast radius and the observable signal that it's safe to proceed.
- **Flags / gating:** {feature flags? canary? percentage rollout?}
- **Rollback trigger:** {condition that reverses rollout} — mirror in `KPI.md` decision rule.
- **Owner / approver:** {person or team}.

## References

Every reference states **how it supports the plan**. Local claims need `file:line`; external claims need a GitHub path/line or PR/commit link.

- [`src/auth/middleware.ts:42`](https://github.com/owner/repo/blob/main/src/auth/middleware.ts#L42) — current behavior the plan extends.
- [owner/repo#123](https://github.com/owner/repo/pull/123) — prior change showing the rollout pattern used here.
```

> **Tip:** When closing an open question, delegate the research loop to `octocode-research`; a resolution without a citation is not resolved.
