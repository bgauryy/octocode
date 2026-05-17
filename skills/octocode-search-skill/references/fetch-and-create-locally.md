# Fetch And Create Locally

Use when the user wants a remote skill to live as a local skill folder on disk — either as a verbatim install or as a starting point for a new, adapted local skill.

## When To Use

- User says "install this skill", "add this skill", "fetch this skill", "save this locally", "copy this skill into my project".
- User wants to adapt a remote skill into their own (`Create A Local Skill From Research` path in `SKILL.md`).
- User wants to inspect a remote skill's full folder locally before deciding.

## Inputs

Normalize one of these into `(owner, repo, branch, source-path, skill-name)`:

```text
owner/repo/path/to/skill
owner/repo/path/to/skill/SKILL.md
https://github.com/<owner>/<repo>/tree/<branch>/<path>
https://github.com/<owner>/<repo>/blob/<branch>/<path>/SKILL.md
```

- Strip trailing `SKILL.md` to get the folder.
- `skill-name` defaults to the final folder segment; allow the user to override.

## Flow

1. Confirm intent with the user: install verbatim, or adapt into a new local skill?
2. Resolve destination(s) using `references/install-reference.md` — provider(s), scope (user vs project vs custom path), project root if project-scoped, and install mode.
3. Fetch the source folder with Octocode:
   - Default: `githubGetFileContent` in directory mode for a single skill folder.
   - Use `githubCloneRepo` when the skill has many sibling assets or the user wants the whole repo for context.
   - Stage downloads under a scratch path the user controls; do not write directly into final destinations yet.
4. Validate the download: the folder contains `SKILL.md` with valid `name` and `description` frontmatter.
5. Safety scan: read `SKILL.md`, then any `scripts/`, install hooks, or executable helpers. Flag risky behavior to the user before writing.
6. If adapting (not verbatim):
   - Build a research synthesis (see §Create A Local Skill From Research in `SKILL.md`).
   - Draft a new `SKILL.md` with the user's own framing and triggers; reuse only patterns the license allows.
   - Cite the source skill in the new skill's footer or initial commit message.
7. Conflict check per destination using `references/install-reference.md` rules; apply the user's per-destination choice (`Overwrite`, `Skip`, `Rename`, `Diff`, `Cancel`).
8. Write into each destination. Prefer atomic copy. Never symlink a fetched download — symlinks are for stable local sources only.
9. Verify each destination: `ls "<destination>/<skill-name>/SKILL.md"` succeeds.
10. Report per-destination success or failure, and how the active runtime picks up the new skill (restart, reload, or auto-scan).

## Cautions

- Do not write fetched `scripts/` or install hooks to disk silently. Flag them to the user before step 8.
- Do not silently rename the skill folder; if the source name conflicts, ask the user.
- If the source license is missing or restrictive, surface that before installing or adapting.
- For adaptations, never copy another skill wholesale unless license and user explicitly allow it.

## Recovery

- Partial download: re-fetch once; if still partial, list missing files and stop.
- Frontmatter invalid after fetch: do not install; report the gap and ask whether to fix locally before writing.
- Permission denied on write: ask whether to switch scope (e.g. project to user) or pick a user-writable destination.
- User changes intent mid-flow (verbatim to adapt, or vice versa): keep the staged download in the scratch path and resume from step 5 or 6.
- Source moved or 404: re-resolve the path; the skill may have been renamed or moved into a `skills/` subfolder.
