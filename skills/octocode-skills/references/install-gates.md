# Install Gates

Load when the user chooses to install. Why: every write needs explicit destination + approval.

An install = copy or symlink a `SKILL.md` folder into a path the runtime scans. No single official installer — any method that lands a valid folder is fine.

## Normalize source

Accept `owner/repo/path`, GitHub tree/blob URLs, absolute/relative paths. Strip trailing `SKILL.md`. `skill-name` = final folder segment unless overridden. If frontmatter `name` ≠ folder, surface mismatch and ask.

Prefer after approval: `npx octocode skill --add --path <src> --platform <hosts> [--mode copy|symlink|hybrid]`.

## Four destination questions

Skip only if already answered this turn:

1. Provider(s)? — one, several, or all agents.
2. Scope per provider? — user / project / custom.
3. If project: which absolute root?
4. Mode? — symlink (stable local source) or copy (portable / remote).

Third-party with `scripts/` or hooks: also ask "Inspect scripts before install?" Default yes.

## Conflict + checklist

Per destination: `ls "<dest>/<skill-name>"` — Overwrite / Skip / Rename / Diff / Cancel. Never silent overwrite.

Checklist: valid frontmatter → safety scan → destinations confirmed → conflict check → explicit approval → fetch if remote → write → `test -f …/SKILL.md` → optional reload hint.

Symlink only when source is stable local, user wants live edits, and runtime supports it. Else copy.
For multi-vendor symlink sync: dry-run `scripts/skill-sync.mjs`, show plan, wait for human, then `--approve` (`references/skill-sync.md`).

Next: when choosing destinations load `references/install-destinations.md`; when syncing vendors load `references/skill-sync.md`; when source is remote load `references/fetch-remote.md`.
