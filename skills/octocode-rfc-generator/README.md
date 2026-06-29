# Octocode RFC Generator

`octocode-rfc-generator` writes evidence-backed RFCs, design docs, architecture proposals, migration plans, implementation plans, and technical decision briefs before coding.

Use it when the cost of being wrong is higher than the cost of writing the decision down.

## When to use

- A change touches shared contracts, multiple packages, infrastructure, security, data migration, or public behavior.
- The user wants alternatives compared before implementation.
- A refactor or migration needs rollout, rollback, and risk notes.
- Research already exists and needs to become a durable proposal.
- The team needs an implementation plan another engineer can follow.

Use `octocode-brainstorming` if the idea still needs validation. Use `octocode-research` if the user wants investigation, review, or code changes now. Use `octocode-roast` for critique rather than planning.

## Features

- Current-state evidence with local `file:line`, GitHub path, PR, commit, package, or formal-source citations.
- At least two alternatives unless the user explicitly asks for a single implementation plan.
- Recommendation tied to evidence, constraints, and tradeoffs.
- Risks, non-goals, unresolved questions, migration notes, and rollback options.
- Implementation steps ordered by dependency rather than preference.
- Optional saved RFC path under `.octocode/rfc/RFC-{meaningful-name}.md` when the user approves saving.

## How it works

The skill follows this flow:

```text
UNDERSTAND -> RESEARCH -> COMPARE OPTIONS -> WRITE RFC / PLAN -> VALIDATE -> DELIVER
```

It starts by clarifying the decision and evidence surfaces. It gathers current-state proof, compares viable approaches, writes the chosen document shape, checks that assumptions and citations are visible, then delivers the RFC or implementation plan in chat or saves it with approval.

## Internal flow

1. Decide whether full RFC mode or a lighter implementation plan is appropriate.
2. Research current behavior, prior art, history, package details, or binary/artifact facts.
3. Build an option table with benefits, costs, risks, and rejection reasons.
4. Write the document using the RFC body and implementation sections.
5. Validate citations, unresolved questions, rollout order, and rollback notes.
6. Report the result and any remaining evidence gaps.

## Installation

Install the published skill:

```bash
npx octocode skill --name octocode-rfc-generator
```

Install from a GitHub path or fork:

```bash
npx octocode skill --add bgauryy/octocode/skills/octocode-rfc-generator
```

## Benefits

- Turns scattered research into a reviewable technical decision.
- Keeps alternatives and tradeoffs visible instead of burying them in implementation.
- Gives implementers a dependency-ordered plan with risks and rollback notes.
- Reduces churn on high-cost changes by making weak assumptions explicit early.

## For developers

Keep `SKILL.md` as the short router for workflow, research playbook, RFC body, and implementation sections. Put detailed decision mechanics in `references/workflow.md`, evidence collection in `references/research-playbook.md`, and document structure in `references/rfc-template.md` plus `references/rfc-implementation.md`. Run `scripts/eval-rfc.mjs --self-test` after prompt or reference changes.
