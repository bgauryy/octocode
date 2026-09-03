# Install Gates

Load when you choose to install. Why: every write needs an explicit destination and approval.

An install = copy or symlink a `SKILL.md` folder into a path the runtime scans. No single official installer — any method that lands a valid folder is fine.

## Normalize source

Accept `owner/repo/path`, GitHub tree/blob URLs, absolute paths, or relative paths. Strip trailing `SKILL.md`. `skill-name` = final folder segment unless overridden. If frontmatter `name` ≠ folder, surface the mismatch, and ask.

Prefer after approval: `npx octocode skill install --add <src> --platform <hosts> [--mode copy|symlink|hybrid]`. This copies the validated source into `<octocode-home>/skills/<name>` as the canonical installation, then creates vendor links from that copy. `--mode copy|hybrid` changes vendor destinations only; canonical home remains a copy.

## Four destination questions

Skip only if already answered this turn:

1. Providers? — one, several, or all agents.
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
