# Octocode RFC Generator

`octocode-rfc-generator` turns research into a reviewable technical decision: an RFC, design doc, migration plan, architecture proposal, or implementation contract. Use it when the cost of being wrong exceeds the cost of writing the reasoning down.

## The Problem

When decisions live only in chat, alternatives disappear, assumptions go untested, and rollback arrives too late. The skill captures current-state evidence, compares viable options, explains tradeoffs, and produces a document another engineer can review or implement.

## Capabilities

- Current-state evidence from local code, GitHub paths, PRs, commits, packages, or formal sources.
- Comparison of viable alternatives and the status quo when relevant.
- Decision language tied to constraints, evidence, tradeoffs, and non-goals.
- Risk, pre-mortem, unresolved-question, migration, rollout, and rollback sections.
- Implementation steps ordered by dependency rather than preference.
- Decision-blocking questions closed with evidence; remaining uncertainty made explicit.
- Success criteria and post-ship verification derived from the RFC's goals.
- Optional companion documents for readiness, implementation, KPIs, and source inventories.

## Operating Model

The workflow is:

```text
UNDERSTAND -> RESEARCH -> PREREQUISITES -> COMPARE -> WRITE -> CLOSE QUESTIONS -> KPI -> VALIDATE -> DELIVER
```

The agent clarifies the decision, gathers evidence, compares options, writes the smallest useful artifact set, closes blockers, defines measurable success where useful, validates, and delivers in chat or as an approved artifact.

## Output

On an approved save, the skill writes the chosen set under `.octocode/rfc/{name}/`:

- **`RFC.md`** — the decision. Reviewer-facing, frozen at decision, and the single source of truth for goals and scope.
- **`PREREQUISITES.md`** — readiness, baselines, blockers, owners, setup, and migration constraints when existing code needs a separate gate.
- **`IMPLEMENTATION.md`** — the build. Closes decision blockers, records explicit deferrals, and orders implementation, verification, and rollback by dependency.
- **`KPI.md`** — acceptance, measurable signals, decision rules, and traceability when success needs its own lifecycle.
- **`RESOURCES.md`** — a source appendix when the inventory would bloat the decision; decisive claims stay cited where they appear.

`RFC.md` is the default. Companion files are added only when their content needs separate ownership or lifecycle.

## User Experience

Users get a review-ready decision with goals, evidence, options, recommendation, risks, rollout or rollback when relevant, and a way to verify success. The skill makes engineering judgment visible rather than replacing it.

It pairs well with `octocode-brainstorming` before the decision exists and `octocode-research` when the decision needs more proof or implementation.

## Installation

Install the published skill with:

```bash
npx octocode skill --name octocode-rfc-generator
```

## Maintainer Notes

Keep this README focused on the decision-document story. Keep the detailed RFC structure, migration mechanics, and validation behavior in the agent-facing skill file and references.

Edit the canonical `skills/octocode-rfc-generator/` source and run the skill review before reporting changes. Build tooling owns generated mirrors.
