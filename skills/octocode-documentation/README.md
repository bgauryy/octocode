# Octocode Documentation

Create, repair, or review documentation for humans and coding agents — with style guidance, Markdown checks, and verified claims.

## Use when

- A README, tutorial, how-to, reference, runbook, ADR, or migration guide is missing or stale.
- Agent instruction files (`AGENTS.md`, `CLAUDE.md`, `COPILOT-INSTRUCTIONS.md`) need restructuring.
- Existing prose needs a factual accuracy or style review.

## Not for

- Architecture decisions that need evidence first → `octocode-research` then `octocode-rfc-generator`
- Skill folder structure or SKILL.md review → `octocode-skills`
- Open-ended ideation about what to build → `octocode-brainstorming`

## Workflow

```text
UNDERSTAND → RESEARCH → CLASSIFY → OUTLINE GATE → WRITE → STYLE → VERIFY
```

The Markdown linter reports ERROR, WARN, and INFO findings. Non-Markdown text still requires a manual style check.

## Install

```bash
npx -y octocode skill install octocode-documentation
```

## Maintainer verification

```bash
node scripts/style-lint.mjs README.md
node scripts/style-lint.mjs --self-test
```

Then run the `octocode-skills` review against this folder.

---

Style guidance: [Google developer documentation style guide](https://developers.google.com/style) · [Diátaxis](https://diataxis.fr/) · [agents.md](https://agents.md/)
