# Create A Local Skill

Load when synthesizing a new local skill from research. Why: plan + approve before writing.

If fetching a remote skill first, load `references/fetch-remote.md`. Shape the lobby with `references/skill-anatomy.md` + `references/skill-improve.md` (workflows in `SKILL.md`, no overlaps).

## Before writing

1. Synthesize: user need, inspected sources, gates, resources, exclusions.
2. Plan: name, destination, trigger draft (`references/description-tuning.md`), workflow outline (goes in lobby), validation.
3. Gate: create / adjust / inspect-more / cancel.

## After approval

Write the lobby (`SKILL.md`) with purpose, workflows, hard rules, stop conditions, and when/why routes. Put depth in one-concept refs. Hooks → `references/hooks-add.md`. Scripts → `references/skill-scripts.md`.

Create `references/references.md` from `references/references-template.md` with sources actually consulted.

Run `node scripts/skill-review.mjs <new-skill-dir>`; clear ERRORs before done.

Next: when writing instruction patterns load `references/skill-authoring.md`; when tuning the trigger load `references/description-tuning.md`.
