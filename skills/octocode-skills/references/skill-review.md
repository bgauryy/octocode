# Skill Review

Load when reviewing, updating, or creating a skill. Why: check structure, routing, prose, and portability before claiming done.

Review combines mechanical findings with judgment about density, ownership, navigation, portability, and working scripts.

## What review covers

1. **Best practices** — lobby owns workflows; progressive disclosure; one owner per concept; scripts over mechanical prose (`references/skill-improve.md`, `references/skill-anatomy.md`).
2. **Quality rubric** — trigger, workflow, gates, evidence, UX, risk (`references/quality-rubric.md`).
3. **Mechanical rules** — frontmatter, missing refs/scripts/schemes, JSON contract shape, length, routing, hooks, prose, and description trigger quality (`description-concise` / `description-rigid` / `description-redundant`) (`references/skill-review-rules.md`).
4. **Navigation** — agents can reach the relevant references, scripts, and schemes, understand when to use them, and continue dependent procedures (`references/skill-anatomy.md`). Do not duplicate an index merely to satisfy a lobby-listing warning.
5. **Standalone** — static path checks (`link-outside-skill`) find literal references; constructed paths and optional integrations need isolated runtime checks.
6. **Cleanup** — every shipped reference, doc, script, and scheme is reachable, coherent, effective, and worth its context/execution cost; no routing wrappers, fallback stubs, duplicates, development-only metadata, probes, or scratch artifacts (`unused-file`, `references/skill-cleanup.md`).

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

For an authorized runtime review, copy the shipped skill alone into a temporary directory, use a separate working directory, and run its documented help and finite fixture checks. Keep sibling skills and developer-only files absent. Test optional integrations both without their dependency (clear setup guidance) and with the documented dependency supplied. Static review does not execute untrusted scripts or prove portability.

## Fix loop

Fix ERRORs first, then assess WARNs with `references/skill-review-rules.md`. Re-run until ERRORs clear; explain any intentional residual WARN.

## Hooks note

Review `hooks-*` covers Claude-style `hooks:` frontmatter. Cursor/Codex native configs must be reviewed directly — outside `SKILL.md`. <!-- style-lint: ignore-line passive-voice -->

Next: when interpreting findings load `references/skill-review-rules.md`; when rating/refactor load `references/self-improvement.md`.
