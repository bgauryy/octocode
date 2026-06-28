# Octocode Skills

`octocode-skills` helps agents work on Agent Skills themselves: finding, evaluating, installing, creating, linting, refactoring, and improving `SKILL.md` folders.

Use it when the subject of the task is a skill, a skill marketplace, or the folder format around `SKILL.md`.

## How it works

The skill identifies the target skill folders, reads `SKILL.md` and the relevant references, then checks trigger clarity, progressive disclosure, README coverage, scripts, install risk, and duplicate content. When editing, it patches the smallest useful surface and reruns `scripts/skill-lint.mjs` plus any skill-specific checks before reporting results.

## Good asks

- "Find a good skill for this task."
- "Evaluate this `SKILL.md` before I install it."
- "Install this skill into Codex or Cursor."
- "Create a local skill from these patterns."
- "Refactor these skill instructions and run the lint."
- "Compare two skill candidates and recommend one."

## What you get

- Filtered recommendations instead of raw search dumps.
- Candidate identity by repo/path or local path.
- Quality judgment based on trigger clarity, workflow, evidence, gates, specificity, portability, and risk.
- An explicit gate before installs, overwrites, symlinks, config changes, or file writes.
- Verification with `scripts/skill-lint.mjs` after skill edits.

## Use another skill when

- The task is normal code research: route to the `octocode`, `octocode-research`, or `octocode-engineer` skill.
- The task is package discovery, not Agent Skills: use the appropriate Octocode package or repo search.
- The user wants to improve a prompt that is not a skill folder: use a prompt-specific workflow instead.

## User value

This skill keeps skill work evidence-based and reversible. It inspects real files before recommending them, avoids blind installs, and makes generated or edited skills lean enough for agents to load.
