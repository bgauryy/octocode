# Octocode Architect

Apply architect-level rigor to consequential code planning, implementation, and review without making small changes ceremonial.

## Use when

- A change affects boundaries, interfaces, data flow, persisted state, or several consumers.
- You need an explicit blast-radius, rollout, rollback, or maintainability review.
- An optimization needs a baseline and measurable result.

## Capabilities

- Models behavior as source → transformation → boundary → sink.
- Cross-checks implementation, callers, runtime wiring, tests, and similar behavior.
- Defines the smallest useful slice and verifies its owned interface.
- Preserves concurrent human or agent work and attributes failures from comparable baselines.
- Scales security, resilience, observability, and migration work to material risk.
- Keeps cleanup task-scoped and repository records consistent.

## Workflow

```text
THINK → PLAN → CODE → REVIEW
```

Small changes stay brief. Consequential changes use explicit interface, impact, verification, and rollback contracts.

## Install

```bash
npx octocode skill install octocode-architect
```
