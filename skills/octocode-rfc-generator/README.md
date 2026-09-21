# Octocode RFC Generator

Turn a consequential technical choice into an evidence-backed RFC, design document, migration plan, architecture proposal, or measurable implementation contract.

## Use when

- Coding before deciding can make the wrong path expensive.
- Viable alternatives, rollout, rollback, or migration need explicit comparison.
- You must reassess an existing RFC against live code.

Use `octocode-brainstorming` first while the worth-building question remains open.

## Capabilities

- Compares viable alternatives and the status quo when relevant.
- Separates goals, non-goals, prerequisites, implementation, KPIs, and sources by ownership.
- Checks completeness and closes decision-blocking questions with evidence.
- Optionally uses two agents to challenge open questions, with `clasify` obtaining a Jev provider judgment over the supplied arguments and the host verifying the result.
- Defines acceptance first, then orders implementation and verification by dependency.
- Defines measurable acceptance, rollout, rollback, and audit reasoning.

## Workflow

```text
UNDERSTAND → RESEARCH / PROVISIONAL COMPARISON → PREREQUISITES → CLOSE DECISION BLOCKERS → DECIDE/CONFIRM → DEFINE ACCEPTANCE → PLAN → VALIDATE → DELIVER
```

Use `RFC.md` for a consequential decision and standalone `PLAN.md` for execution of an already-settled decision. Add `PREREQUISITES.md`, `IMPLEMENTATION.md`, `KPI.md`, or `RESOURCES.md` only when the content needs a separate lifecycle.

## Install

```bash
npx -y octocode skill install octocode-rfc-generator
```

## Optional semantic assessment

The `clasify` route is built into this skill. Two agents first provide independent arguments and one rebuttal each. The tool calls the Jev provider only if their positions still differ, evidence/direct checks cannot settle the issue, and different judgments change the next action. It prioritizes bounded risk; it is not presumed to improve accuracy. The host retains responsibility for evidence and the RFC decision.

This step needs host subagents and `OCTOCODE_CLASSIFICATION_API` in the Octocode process or MCP environment. The MCP tool is registered only when that key is configured; a direct CLI call without it must report the missing key. Without either capability, use the ordinary evidence-based RFC workflow. Cheap checks and settled plans do not need a debate. See [the review protocol](references/jev-review.md) for limits and result handling.

## Maintainer verification

Run `node scripts/validate-rfc.mjs <file-or-folder>` for readiness or `--draft` for investigation. Run `node scripts/validate-debate.mjs --self-test` for judge admission and packet integrity, `node scripts/validate-review-cost.mjs --self-test` for cost receipts, then run the `octocode-skills` review against this folder.
