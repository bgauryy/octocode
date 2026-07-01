---
name: octocode-skills
description: "Use when the task is about Agent Skills or SKILL.md folders — find, evaluate, lint, rate, improve, install, or create them across local paths, GitHub, and marketplaces. Also triggers on skill descriptions, trigger tuning, skill quality, refactoring, and install targets."
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

Use Octocode for skill research: MCP tools if available, else the CLI. Read `references/octocode.md` when choosing transport or schema-exact calls. For public queries, run web search in parallel, then confirm each lead against the real `SKILL.md`.

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
- `references/skill-lint.md` — when linting structure; it documents `scripts/skill-lint.mjs`.
- `references/install-reference.md` — when installing or choosing targets/scopes.
- `references/fetch-and-create-locally.md` — when fetching a remote skill into a local folder.
- `references/create-local-skill.md` — when creating or synthesizing a local skill.
- `references/recovery.md` — when search, fetch, install, or marketplace access fails.

## Scripts

`scripts/skill-lint.mjs` — lint skill structure, routing, scripts, and prompt quality. Run it before reporting any created or edited skill done.

## Installation

When installing, read `references/install-reference.md`: inspect `scripts/` and hooks before copying third-party skills; confirm provider, scope, copy-vs-symlink, conflicts, and verification plan.

When creating, read `references/create-local-skill.md`: synthesize from evidence, write lean files, include a `references/references.md` audit trail using `references/references-template.md`, then run `scripts/skill-lint.mjs` before reporting done.
