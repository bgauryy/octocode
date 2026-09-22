# Octocode RFC Generator

Turn a consequential technical choice into an evidence-backed RFC, design document, migration plan, architecture proposal, or measurable implementation contract.

## Use when

- An architecture, migration, public-contract, or multi-phase change needs a reviewed decision before work begins.
- The right outcome requires evidence, stakeholder input, prerequisites, defined acceptance, and a rollback plan.
- A complex plan needs separation between the decision (RFC) and the execution (PLAN).

## Not for

- Open-ended ideation before a decision is formed → `octocode-brainstorming`
- Trivial edits or single-file changes → make the change directly
- Implementation of an already-settled decision → use `octocode-architect` or a plain plan

## Workflow

```text
UNDERSTAND → RESEARCH → PREREQUISITES → CLOSE BLOCKERS → DECIDE → DEFINE ACCEPTANCE → PLAN → VALIDATE → DELIVER
```

Use `RFC.md` for a consequential decision and standalone `PLAN.md` for execution of an already-settled decision. Add supplementary files (`PREREQUISITES.md`, `IMPLEMENTATION.md`, `KPI.md`) only when they have a separate lifecycle.

## Install

```bash
npx -y octocode skill install octocode-rfc-generator
```

## Maintainer verification

```bash
node scripts/validate-rfc.mjs <file-or-folder>
node scripts/validate-debate.mjs --self-test
```

Then run the `octocode-skills` review against this folder.
