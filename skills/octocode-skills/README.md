# Octocode Skills

`octocode-skills` helps agents work on Agent Skills themselves: finding, evaluating, installing, creating, linting, refactoring, and improving `SKILL.md` folders.

Use it when the task is about a skill, a skill marketplace, install target, trigger description, README, reference map, script helper, or skill publication quality.

## When to use

- Find or compare skills for a task.
- Evaluate a third-party `SKILL.md` before installing it.
- Install a skill into a provider or project scope.
- Create a local skill from researched patterns.
- Refactor a skill into a lean `SKILL.md` plus focused references.
- Tune a skill description so it triggers at the right time.
- Add or improve README documentation for publication.
- Run structural linting before shipping a skill.

Use `octocode` or `octocode-research` for normal code research. Use a prompt-specific workflow when the target is not an Agent Skill folder.

## Features

- Skill discovery across local paths, GitHub paths, and marketplace-style sources.
- Candidate inspection from real `SKILL.md` files instead of summaries alone.
- Quality rubric for trigger clarity, workflow, evidence, gates, output UX, specificity, portability, and risk.
- Description tuning for better activation without keyword stuffing.
- Install planning across provider, scope, copy-vs-symlink mode, conflicts, and verification.
- Skill creation from evidence with references and audit trail.
- Mechanical linting through `scripts/skill-lint.mjs`.
- README checks for overview, features, how-it-works, audience, and `npx octocode skill ...` installation.
- Explicit gates before installs, overwrites, symlinks, config changes, or file writes.

## How it works

The skill follows this flow:

```text
UNDERSTAND -> DISCOVER -> INSPECT -> JUDGE -> RECOMMEND -> USER GATE -> ACT -> VERIFY
```

It first identifies the target skill or search space, inspects real files, applies the quality rubric, asks for approval before risky writes or installs, performs the smallest useful action, and verifies with the linter or target-specific checks.

## Internal flow

1. Resolve the skill identity by absolute path, repo/path, marketplace result, or install input.
2. Read `SKILL.md` and only the behavior-affecting references or scripts needed for the request.
3. Judge progressive disclosure, trigger fit, gates, output shape, scripts, hooks, README coverage, and install risk.
4. For installs, confirm provider, scope, destination, mode, conflicts, and script inspection before writing.
5. For edits, preserve the skill's intent while moving conditional detail into references and deterministic work into scripts.
6. Re-run `scripts/skill-lint.mjs` and report residual warnings or risks.

## Installation

Install the published skill:

```bash
npx octocode skill --name octocode-skills
```

Install from a GitHub path or fork:

```bash
npx octocode skill --add bgauryy/octocode/skills/octocode-skills
```

When installing third-party skills, inspect `SKILL.md`, bundled scripts, hooks, and destination conflicts before copying or symlinking.

## Benefits

- Avoids blind skill installs and stale marketplace assumptions.
- Helps authors keep `SKILL.md` lean enough for agents to load reliably.
- Makes published skills easier for users to understand before installation.
- Provides a deterministic lint gate for structure, prompt hygiene, routing, scripts, hooks, and README coverage.

## For developers

Developers maintain behavior in `SKILL.md`, focused `references/`, and deterministic `scripts/`. `scripts/skill-lint.mjs` is the quality gate for structure, prompt hygiene, README coverage, script routing, hook handling, and installation docs. Keep repeated procedures in scripts, keep conditional detail in references, and keep `SKILL.md` within the lint line budget.
