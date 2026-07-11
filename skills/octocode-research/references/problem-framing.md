# Problem Framing

Load after `algorithm.md`, before deep research or edits. Classify the request from evidence, not the user's label.

## Problem Contract

Capture one compact receipt:

```text
actual | expected/desired | authority | trigger | impact | success criteria | non-goals
```

Authority is a test, specification, API/schema, documented support promise, accepted user criterion, or established behavior. If authority or actual behavior is unknown, keep the class `unknown` and investigate before patching.

## Classification

| Class | Evidence test | Route |
|---|---|---|
| bug | supported current contract is violated under relevant conditions | reproduce → `workflow-debug.md` |
| feature | desired capability creates a new contract | capability gap/options → acceptance tests → `workflow-change.md` |
| enhancement | current contract holds but a measurable quality target should improve | baseline → bottleneck → target/experiment → `workflow-change.md` |
| unknown | actual, authority, or desired outcome is unresolved | exact reads/runtime evidence → classify or ask |

Features use rationale, constraints, consumers, and acceptance criteria; root cause is reserved for supported contract violations. Enhancements require a baseline and target.

## System Model

Before choosing a fix, map only the load-bearing path:

```text
entry/input → transformations → state/dependency boundaries → side effects/output → consumers
```

Name invariants at each relevant boundary.
For a bug, find the first boundary where actual behavior diverges from the contract.
For a feature or enhancement, find the smallest boundary that can own the new criterion while preserving existing invariants.

## Completion Tests

- Bug: reproduction or equivalent evidence, causal mechanism/trigger, alternate disconfirmed, regression check.
- Feature: capability gap, affected consumers, explicit acceptance tests, compatibility decision.
- Enhancement: measured baseline, target metric, experiment/change, regression guard.
- Unknown: one missing fact and the cheapest check or focused user question that resolves classification.

Next: choose the matching route in `workflows.md`; use `code-research.md` for proof depth.
