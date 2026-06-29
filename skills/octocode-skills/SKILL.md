---
name: octocode-skills
description: "Use when the task is about Agent Skills or SKILL.md folders: find, compare, evaluate, lint, rate, improve, refactor, install, preview, synthesize, or create skills from local paths, GitHub, or marketplaces. Trigger on skill descriptions, trigger tuning, skill quality, or skill installation targets."
---

# Octocode Skills

Evaluate, lint, install, and author Agent Skills: folders with `SKILL.md` plus optional `references/`, `scripts/`, and `assets/`. Agents load skills progressively, so keep `SKILL.md` as the compact operating map and route conditional detail out.

Flow: `UNDERSTAND -> DISCOVER -> INSPECT -> JUDGE -> RECOMMEND -> USER GATE -> ACT -> VERIFY`.

Hard rules: inspect real `SKILL.md` content before recommending, adapting, installing, or quoting it; identify candidates by repo/path or absolute path; gate installs, writes, overwrites, symlinks, and config changes; filter results instead of handing the user raw search dumps.

Stop when one inspected recommendation fits, two High-quality candidates establish a clear top pick, three search angles add nothing new, or a user gate is pending.

## Tooling

Use Octocode for skill research: MCP tools if available, otherwise the CLI. Read `references/octocode.md` when choosing transport or schema-exact calls. For public skill queries, also run web search in parallel, then confirm any lead by inspecting the real `SKILL.md`.

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

- `scripts/skill-lint.mjs` — lint skill structure, routing, scripts, and prompt quality.

## Installation

When installing, read `references/install-reference.md`: inspect `scripts/` and hooks before copying third-party skills; confirm provider, scope, copy-vs-symlink, conflicts, and verification plan.

When creating, read `references/create-local-skill.md`: synthesize from evidence, write lean files, include a `references/references.md` audit trail using `references/references-template.md`, then run `scripts/skill-lint.mjs` before reporting done.
