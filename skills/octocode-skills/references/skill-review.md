# Skill Review

Load when reviewing, updating, or creating a skill. Why: check structure, routing, prose, and portability before claiming done.

Review combines mechanical findings with judgment about density, ownership, navigation, portability, and working scripts.

## What review covers

1. **Best practices** — lobby owns workflows; progressive disclosure; one owner per concept; scripts over mechanical prose (`references/skill-improve.md`, `references/skill-anatomy.md`).
2. **Quality rubric** — trigger, workflow, gates, evidence, UX, risk (`references/quality-rubric.md`).
3. **Mechanical rules** — frontmatter, missing refs/scripts, length, routing, hooks, prose, description trigger quality (`description-concise` / `description-rigid` / `description-redundant`) (`references/skill-review-rules.md`).
4. **Navigation** — lobby lists every reference and script with when/how plus the workflow, routes carry when/why, chunks declare entry and next hop, flow phases are routed (`references/skill-anatomy.md`).
5. **Standalone** — every local file reference stays inside the folder (`link-outside-skill`).
6. **Cleanup** — every shipped file is reachable and useful; no duplicate, development-only metadata, probe, or scratch artifacts (`unused-file`, `references/skill-cleanup.md`).

## Run

```bash
node scripts/skill-review.mjs                       # every skill under nearest skills/ root
node scripts/skill-review.mjs ../skills             # every immediate child skill in a collection
node scripts/skill-review.mjs ../some-skill         # one or more folders
node scripts/skill-review.mjs ../some-skill --json  # machine-readable
node scripts/skill-review.mjs --self-test            # collection/error/frontmatter regressions
```

Exit `1` on any ERROR; WARN is advisory. Always run before reporting create/edit done; surface findings.
`scripts/skill-lint.mjs` is a compatibility alias for the same command.

No-arg scan is relative to this skill copy: `.agents/skills/octocode-skills` scans `.agents/skills`; packaged `skills/octocode-skills` scans `skills`.

## Fix loop

Fix ERRORs first, then assess WARNs with `references/skill-review-rules.md`. Re-run until ERRORs clear; explain any intentional residual WARN.

## Hooks note

Review `hooks-*` covers Claude-style `hooks:` frontmatter. Cursor/Codex native configs must be reviewed directly — outside `SKILL.md`.

Next: when interpreting findings load `references/skill-review-rules.md`; when rating/refactor load `references/self-improvement.md`.
