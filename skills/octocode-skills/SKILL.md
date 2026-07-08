---
name: octocode-skills
description: "Use when working with Agent Skills or SKILL.md folders: find, evaluate, lint, rate, improve, install, or create them across local paths, GitHub, and marketplaces — including description tuning and install targets."
---

# Octocode Skills

Evaluate, lint, install, and author Agent Skills: folders with `SKILL.md` plus optional `references/`, `scripts/`, and `assets/`. Agents load skills progressively, so keep `SKILL.md` as the compact operating map and route conditional detail out.
Flow: `UNDERSTAND -> DISCOVER -> INSPECT -> JUDGE -> RECOMMEND -> USER GATE -> ACT -> VERIFY`.

Hard rules:
- Inspect real `SKILL.md` content before recommending, adapting, installing, or quoting.
- Identify each candidate by repo/path or absolute path; filter results, never hand over raw search dumps.
- Gate every install, write, overwrite, symlink, and config change behind user approval.

Stop when one inspected recommendation fits, two High-quality candidates establish a clear top pick, three search angles add nothing new, or a user gate is pending.

## Tooling

For Octocode-backed skill research, load `references/octocode.md` and delegate search mechanics to `octocode-research` when installed. Keep this skill focused on evaluating, linting, improving, and installing the skill folders found.

## Reference Map

Load one reference on demand; each route states when it applies.

- `references/search-playbook.md` — when discovering candidates.
- `references/discovery-surfaces.md` — when shopping beyond raw GitHub.
- `references/quality-rubric.md` — when judging workflow quality.
- `references/quality-signals.md` — when ranking by evidence beyond stars.
- `references/output-format.md` — when presenting results or deep-dives.
- `references/agent-skills-guide.md` — when evaluating, improving, or authoring a skill.
- `references/description-tuning.md` — when optimizing a `description` trigger.
- `references/self-improvement.md` — when rating, reviewing, refactoring, or linting a skill.
- `references/skill-lint.md` — when linting, updating, or creating a skill (new or existing); documents `scripts/skill-lint.mjs`.
- `references/hooks.md` — when adding, reviewing, or explaining a skill's lifecycle hooks; routes to `assets/hooks/` templates.
- `references/install-reference.md` — when installing or choosing targets/scopes.
- `references/fetch-and-create-locally.md` — when fetching a remote skill into a local folder.
- `references/create-local-skill.md` — when creating or synthesizing a local skill.
- `references/recovery.md` — when search, fetch, install, or marketplace access fails.

## Scripts

- `scripts/skill-lint.mjs` — lint skill structure, routing, scripts, and prompt quality; run before reporting any created or edited skill done.
- `assets/hooks/` — copy-paste hook templates; read `references/hooks.md` before wiring a new hook.

## Installation

- Installing: read `references/install-reference.md` — inspect `scripts/` and hooks before copying third-party skills; confirm provider, scope, copy-vs-symlink, conflicts, and verification plan.
- Creating: read `references/create-local-skill.md` — synthesize from evidence, write lean files, include a `references/references.md` audit trail using `references/references-template.md`, then run `scripts/skill-lint.mjs` before reporting done.
