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
- Risk, pre-mortem, unresolved-question, migration, rollout, and rollback sections.
- Implementation steps ordered by dependency rather than preference.
- Open questions closed with Octocode citations, not guesses.
- Success criteria and post-ship verification derived from the RFC's goals.
- Resources and references appendices for local refs, prior art, papers, packages, research artifacts, and search prompts.
- Saved RFC flow when the user wants a durable artifact in the repo.

## Operating Model

The workflow is:

```text
UNDERSTAND -> RESEARCH (octocode) -> PREREQUISITES? -> COMPARE OPTIONS -> WRITE RFC -> CLOSE OPEN QUESTIONS (octocode) -> DERIVE KPIs -> VALIDATE -> DELIVER
```

The agent first clarifies the decision and the evidence surfaces. It gathers proof with Octocode, compares options, writes the RFC, closes every open question with a citation, derives measurable success criteria, validates, then delivers in chat or as an approved repo artifact.

## Output

On an approved save the skill writes a folder `\.octocode/rfc/{name}/` with a document set for different readers and lifecycles:

- **`RFC.md`** — the decision. Reviewer-facing, frozen at decision, and the single source of truth for goals and scope.
- **`PREREQUISITES.md`** — existing-code RFCs only. Written before the implementation plan with current-state evidence, baseline checks, blockers, owners, setup, and migration constraints.
- **`IMPLEMENTATION.md`** — the build. Closes every RFC open question via Octocode research, then a dependency-ordered plan with a test/verification plan and rollback.
- **`KPI.md`** ("Success & Verification") — how to check the RFC and its implementation after shipping: user stories, Gherkin acceptance criteria, measurable signals, a decision rule, and a traceability matrix that binds the document set and detects drift.
- **`RESOURCES.md`** — the refs appendix. Local code refs, external prior art, papers, package links, research artifacts, and useful search prompts; decisive claims are still cited where they appear.

For a small, reversible, single-package change, the skill produces only `RFC.md` with an inline plan, acceptance criteria, and references.

## User Experience

Users should get a document set that feels ready for review: summary, goals/non-goals, evidence, options, recommendation, resources/refs, risks, rollout, rollback, implementation order, and a way to verify success. The skill is not meant to replace engineering judgment; it makes that judgment visible.

It pairs well with `octocode-brainstorming` before the decision exists and `octocode-research` when the decision needs more proof or implementation.

## Installation

Install the published skill with:

```bash
npx octocode skill --name octocode-rfc-generator
```

## Maintainer Notes

Keep this README focused on the decision-document story. Keep the detailed RFC structure, migration mechanics, and validation behavior in the agent-facing skill file and references.

When changing this skill, edit the repo-root `skills/octocode-rfc-generator/` copy (canonical source), then run `node packages/octocode-pi-extension/scripts/build.mjs` to sync the pi-extension mirrors. Before reporting done, run `node scripts/eval-rfc.mjs --self-test` and the smoke prompts in `evals/prompts.md`, and run the skill linter until it reports 0 errors.
