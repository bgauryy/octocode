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

## Compare options

Always include at least two alternatives unless the user explicitly asks for a single implementation plan.

Useful alternatives: do nothing / defer; minimal patch; incremental migration; full redesign; adopt package/library; build in-house; hybrid/phased rollout.

Compare on: fit with current architecture; blast radius; compatibility and migration cost; operational risk; performance/security/data implications; maintenance and ownership; reversibility.

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
- No claim relies on "common practice" without explaining why it applies here.

Reasoning traps:
- First-option bias: search for evidence against the preferred approach.
- False dichotomy: consider hybrids/phased plans.
- Local-vs-external conflict: local constraints usually win; document the tradeoff.
- Metrics claims: use external tools or mark as approximation.

## Deliver

Start with a concise summary:

```text
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
