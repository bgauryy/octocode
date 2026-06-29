# Octocode RFC Generator

`octocode-rfc-generator` gives an agent the structure to turn research into a reviewable technical decision. It writes RFCs, design docs, migration plans, implementation plans, architecture proposals, and decision briefs before risky work begins.

Use it when the cost of being wrong is higher than the cost of writing the reasoning down.

## The Problem

Complex changes fail when the decision lives only in chat. Alternatives disappear, assumptions go untested, rollout order becomes vibes, and rollback is discovered too late.

This skill makes the agent capture current-state evidence, compare viable options, explain tradeoffs, and produce a document another engineer can review or implement.

## Capabilities

- Current-state evidence from local code, GitHub paths, PRs, commits, packages, or formal sources.
- Alternative comparison before recommendation, unless the user explicitly asks for a single plan.
- Decision language tied to constraints, evidence, tradeoffs, and non-goals.
- Risk, unresolved-question, migration, rollout, and rollback sections.
- Implementation steps ordered by dependency rather than preference.
- Saved RFC flow when the user wants a durable artifact in the repo.

## Operating Model

The workflow is:

```text
UNDERSTAND -> RESEARCH -> COMPARE OPTIONS -> WRITE RFC / PLAN -> VALIDATE -> DELIVER
```

The agent first clarifies the decision and the evidence surfaces. It gathers proof, compares options, writes the right document shape, validates citations and open questions, then delivers the RFC or plan in chat or as an approved repo artifact.

## User Experience

Users should get a document that feels ready for review: summary, context, evidence, options, recommendation, risks, rollout, rollback, and implementation order. The skill is not meant to replace engineering judgment; it makes that judgment visible.

It pairs well with `octocode-brainstorming` before the decision exists and `octocode-research` when the decision needs more proof or implementation.

## Installation

Install the published skill with:

```bash
npx octocode skill --name octocode-rfc-generator
```

## Maintainer Notes

Keep this README focused on the decision-document story. Keep the detailed RFC structure, migration mechanics, and validation behavior in the agent-facing skill file and references.
