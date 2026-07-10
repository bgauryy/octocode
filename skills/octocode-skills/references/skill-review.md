# Skill Review

Load when reviewing, updating, or creating a skill — or right after editing any `SKILL.md`. Why: **review is the done-gate** — best practices + structure + routing + prose quality before claiming done.

Review is not a narrow syntax check. It judges whether the skill is efficient for agents: lean lobby, one-concept refs, when/why routes, no overlaps, standalone ship shape, and working scripts.

## What review covers

1. **Best practices** — lobby owns workflows; progressive disclosure; one owner per concept; scripts over mechanical prose (`references/skill-improve.md`, `references/skill-anatomy.md`).
2. **Quality rubric** — trigger, workflow, gates, evidence, UX, risk (`references/quality-rubric.md`).
3. **Mechanical rules** — frontmatter, missing refs/scripts, length, routing, hooks, prose, description trigger quality (`description-concise` / `description-rigid` / `description-redundant`) (`references/skill-review-rules.md`).
4. **Cleanup** — orphans, dupes, dead files (`references/skill-cleanup.md`).

## Run

```bash
node scripts/skill-review.mjs                       # every skill under nearest skills/ root
node scripts/skill-review.mjs ../some-skill         # one or more folders
node scripts/skill-review.mjs ../some-skill --json  # machine-readable
```

Exit `1` on any ERROR; WARN is advisory. Always run before reporting create/edit done; surface findings.
`scripts/skill-lint.mjs` is a compatibility alias for the same command.

No-arg scan is relative to this skill copy: `.agents/skills/octocode-skills` scans `.agents/skills`; packaged `skills/octocode-skills` scans `skills`.

## Fix loop

1. Fix ERRORs first — message names the exact gap.
2. WARNs: match finding → fix (see `references/skill-review-rules.md`).
3. Re-run until ERRORs clear; residual WARNs = gated decision with user.

## Hooks note

Review `hooks-*` covers Claude-style `hooks:` frontmatter. Cursor/Codex native configs must be reviewed directly — outside `SKILL.md`.

Next: when interpreting findings load `references/skill-review-rules.md`; when rating/refactor load `references/self-improvement.md`.
