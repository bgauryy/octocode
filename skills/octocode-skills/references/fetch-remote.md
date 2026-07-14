# Fetch Remote Skill

Load when installing or adapting a remote skill into a local folder. Why: fetch → scan → gate before any destination write.

## When

User says install/add/fetch/save locally, or wants to adapt a remote skill, or inspect the full folder before deciding.

## Normalize

`owner/repo/path`, tree/blob URLs → `(owner, repo, branch, path, skill-name)`. Strip trailing `SKILL.md`. Name defaults to final segment.

## Flow

1. Confirm intent: verbatim install vs adapt.
2. Resolve destinations via `install-gates.md` + `install-destinations.md`.
3. Fetch: inspect via `octocode.md` first; then `npx octocode clone owner/repo/path[@branch]` (or whole repo if many siblings). Stays in Octocode tmp — don't write destinations yet.
4. Validate: folder has `SKILL.md` with `name` + `description`.
5. Safety scan: read `SKILL.md`, `scripts/`, hooks — flag risk before write.
6. If adapting: synthesize via `create-local-skill.md`; reuse only license-allowed patterns; cite source.
7. Conflict check per destination; apply user choice.
8. Write (copy only — never symlink a fetch). Verify `test -f …/SKILL.md`.
9. Report per-destination result + how the runtime reloads skills.

## Cautions

Don't write fetched scripts/hooks silently. Don't silent-rename. Surface missing/restrictive licenses. Never wholesale-copy without license + user OK.

Partial download → re-fetch once then stop. Invalid frontmatter → don't install. Intent flip mid-flow → keep scratch, resume from scan/adapt.

Next: when adapting load `references/create-local-skill.md`; when installing verbatim load `references/install-gates.md`; on 404/permission load `references/recovery.md`.
