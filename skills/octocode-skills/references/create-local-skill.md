# Create A Local Skill From Research

Load when the user chooses to create a skill from findings or asks to synthesize one. Read `agent-skills-guide.md` before planning. If the source is a remote skill being fetched into a local folder, also read `fetch-and-create-locally.md`.

## Before writing files

1. Build a research synthesis:
   - User need and constraints.
   - Inspected source skills and useful patterns.
   - Quality and UX gates to include.
   - Resources to create, if any.
   - Exclusions: copied, generic, risky, or unnecessary pieces.
2. Present a short plan:
   - Skill name and destination.
   - Trigger description draft (see `description-tuning.md`).
   - Workflow outline.
   - Resources and validation plan.
3. Ask for approval with create, adjust, inspect-more, or cancel options.

## After approval

Write the skill with a concise purpose, workflow, tool and resource rules, gates, output UX, and recovery paths. Add `references/`, `scripts/`, or `assets/` only when they reduce repeated work or keep `SKILL.md` lean. Defer to a dedicated skill-creation skill when one is available.

MUST also create `references/references.md` inside the new skill folder using the shape in `references-template.md`. Populate it with every source actually consulted — not sources you did not check. This file is a research audit trail, not a bibliography template.

MUST run `scripts/skill-lint.mjs <new-skill-dir>` (see `skill-lint.md`) and clear ERRORs before reporting the skill as done; report residual WARNs as a gated decision with the user.
