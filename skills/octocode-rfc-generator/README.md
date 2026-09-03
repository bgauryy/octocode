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
- Closes decision-blocking questions with evidence.
- Orders implementation and verification by dependency.
- Defines measurable acceptance, rollout, rollback, and audit reasoning.

## Workflow

```text
UNDERSTAND → RESEARCH → PREREQUISITES → COMPARE → WRITE → CLOSE QUESTIONS → KPI → VALIDATE → DELIVER
```

`RFC.md` is the default decision artifact. Add `PREREQUISITES.md`, `IMPLEMENTATION.md`, `KPI.md`, or `RESOURCES.md` only when the content needs a separate lifecycle.

## Install

```bash
npx octocode skill install octocode-rfc-generator --platform codex
```

## Maintainer verification

Validate the document contract, then run the `octocode-skills` review against this folder.
