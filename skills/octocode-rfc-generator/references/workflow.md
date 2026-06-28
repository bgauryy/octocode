# RFC / Plan Workflow

The full flow, from understanding the ask to delivering the document.

```text
UNDERSTAND → RESEARCH → COMPARE OPTIONS → WRITE RFC / PLAN → VALIDATE → DELIVER
```

Default output location when saving is approved: `.octocode/rfc/RFC-{meaningful-name}.md`.

## Pick mode

| User asks for | Mode | Output |
|---|---|---|
| "write RFC", "design doc", "proposal", "architecture decision" | RFC | Full RFC with alternatives, rationale, risks, implementation plan |
| "plan this work", "research and build", "implementation plan" | Plan | Evidence-backed implementation plan; RFC sections included only when useful |
| "compare approaches", "should we use X or Y" | Decision | Options matrix + recommendation + adoption/rollback notes |
| "migration plan" | Migration | Current state, target state, compatibility, rollout, rollback, phases |
| "validate this RFC/design" | Validation | Claim-by-claim verdict with evidence and gaps |

If the task is a trivial one-file edit with no design choice, say an RFC is unnecessary and suggest using `octocode-engineer` directly.

## Understand

Capture this before research gets broad:

- Problem in one or two sentences.
- Why this needs a decision/plan.
- Affected users, packages, APIs, teams, or workflows.
- Constraints: compatibility, performance, security, rollout, tech stack.
- What "do nothing" costs.
- What evidence is needed to decide.

Ask if the problem or desired output mode is unclear.

## Brainstorming handoff intake

If input includes an `octocode-brainstorming` RFC handoff, normalize it before research:

| Handoff field | RFC use |
|---|---|
| Problem | Motivation and affected users/workflows |
| Chosen framing + value thesis | Summary and Rationale |
| Surviving evidence links | Evidence Summary, Current State, References |
| Alternatives | Alternatives Considered |
| Constraints + risks | Drawbacks, rollout, rollback, open questions |
| Bounded MVP / first slice | Implementation Plan |
| Success signal | Acceptance Criteria |

If no handoff exists, build the same packet from Understand. If problem, decision, constraints, or success signal are missing, ask one focused question or label the output `Draft` with the gap.

## Claim ledger

Maintain a small private ledger while researching:

```text
claim | evidence | confidence | RFC section | gap / next proof
```

Use confidence `confirmed`, `likely`, or `uncertain`. Recommendations may rely only on confirmed/likely claims. Uncertain claims belong in Risks, Open Questions, or Future Possibilities.

## Compare options

Always include at least two alternatives unless the user explicitly asks for a single implementation plan.

Useful alternatives: do nothing / defer; minimal patch; incremental migration; full redesign; adopt package/library; build in-house; hybrid/phased rollout.

Compare on: fit with current architecture; blast radius; compatibility and migration cost; operational risk; performance/security/data implications; maintenance and ownership; reversibility.

## Hard gates

Stop and ask, narrow, or clearly mark `Draft` before delivery when:

- Scope contains multiple independent decisions -> split into RFCs or phases.
- Current state has no real evidence -> research more or mark as unproven.
- Only one option is considered and the user did not ask for a single-path plan -> add do-nothing, minimal patch, or phased rollout.
- Public API/data/security/compatibility changes lack rollout, rollback trigger, and owner/approver.
- Brainstorming handoff says `not ready`, `Prototype First`, `Narrow`, `Park`, or `Do Not Build`.

## Write the plan

For full RFCs use the template references. For implementation plans, include only the sections needed for the work:

```markdown
# Plan: <title>

## Goal
## Evidence Summary
## Current State
## Proposed Approach
## Alternatives Considered
## Step-by-Step Implementation
## Files / APIs / Contracts Touched
## Test and Verification Plan
## Rollout / Migration / Rollback
## Risks and Open Questions
```

Implementation steps should be ordered by dependency, not preference. Avoid time estimates.

## Validate

Before delivering, check:

- Problem and motivation are specific.
- Current state has real evidence.
- Alternatives are fairly compared.
- Recommendation follows from evidence.
- Drawbacks and migration costs are explicit.
- Blast radius is mapped for shared symbols/contracts.
- Risks have mitigations or open questions.
- Implementation plan is actionable and verifiable.
- Every non-obvious claim, recommendation, and rejected alternative has a citation or is marked uncertain.
- No recommendation depends on an `uncertain` ledger claim.
- Implementation-ready docs include acceptance criteria, verification commands, success signal, and rollback trigger.
- No claim relies on "common practice" without explaining why it applies here.

Reasoning traps:
- First-option bias: search for evidence against the preferred approach.
- False dichotomy: consider hybrids/phased plans.
- Local-vs-external conflict: local constraints usually win; document the tradeoff.
- Metrics claims: use external tools or mark as approximation.

## Deliver

Start with a concise summary:

```text
Status: <Draft|Ready for Review|Blocked>
Decision: <recommendation>
Why: <1-2 evidence-backed reasons>
Alternatives: <count and names>
Risk: <low|medium|high + why>
Next step: <one action>
```

Then ask whether to save the full RFC/plan.

- If yes: save to `.octocode/rfc/RFC-{meaningful-name}.md`.
- If no: keep it in chat.
- If user wants implementation: hand off to the agent's normal engineering/edit workflow using the implementation plan.
