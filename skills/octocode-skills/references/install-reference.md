# Install Reference

Use only after the user chooses to install a skill. This reference is provider-agnostic and tool-agnostic. Treat the steps below as a checklist, not a script: apply what fits, deepen what matters.

## What An Install Is

A skill install is: copy or symlink a folder containing `SKILL.md` (plus any `references/`, `scripts/`, `assets/`) into a destination an agent runtime loads at startup.

There is no "official" installer. Any method that produces a valid skill folder at a destination the runtime scans is a valid install.

## Inputs Accepted

Normalize any of these to `(source-folder, skill-name)`:

```text
owner/repo/path/to/skill
owner/repo/path/to/skill/SKILL.md
https://github.com/<owner>/<repo>/tree/<branch>/<path>
https://github.com/<owner>/<repo>/blob/<branch>/<path>/SKILL.md
/absolute/path/to/skill[/SKILL.md]
./relative/path/to/skill[/SKILL.md]
```

Rules:

- Strip trailing `SKILL.md` to get the folder.
- `skill-name` = final folder segment, unless the user overrides it.
- If frontmatter `name` differs from folder name, surface the mismatch and ask.

## Octocode CLI Shortcut

After the user approves an install and Octocode CLI is available, prefer:

```bash
npx octocode skill --add --path <source-folder-or-skills-dir> --platform <platforms> [--mode copy|symlink|hybrid]
```

Use this when an agent already knows a local bundled skills path; the path may be one skill folder, `SKILL.md`, or a skills library root.

## Destinations: Provider Scopes

Most agents load skills from at least two scopes: a user-level scope (applies everywhere) and a project-level scope (applies to one repo / workspace).

Some agents also honor a custom path via env var or config.

Use this matrix as a default — verify with the active runtime when the user is unsure.

| Provider | User scope (global) | Project scope (per-repo) | Custom override |
|---|---|---|---|
| shared agents | `~/.agents/skills/` | `<repo>/.agents/skills/` | user-supplied absolute path |
| claude-code | `~/.claude/skills/` | `<repo>/.claude/skills/` | env or config `skillsDestDir` |
| claude-desktop | `~/.claude-desktop/skills/` | n/a | runtime config dir |
| cursor | `~/.cursor/skills/` | `<repo>/.cursor/skills/` | runtime config |
| codex | `~/.agents/skills/` | `<repo>/.agents/skills/` | admin `/etc/codex/skills` |
| opencode | `~/.config/opencode/skills/` | `<repo>/.opencode/skills/` | runtime config |
| pi | `~/.pi/agent/skills/` | `<repo>/.pi/skills/` | runtime config |
| copilot | `~/.copilot/skills/` | `<repo>/.github/skills/` | `/skills add` in Copilot CLI |
| gemini | `~/.gemini/skills/` or `~/.agents/skills/` | `<repo>/.gemini/skills/` or `<repo>/.agents/skills/` | `/skills link` / `gemini skills` |
| other / custom | any directory the runtime scans | any in-repo path | user-supplied absolute path |

Windows equivalents replace `~` with `%USERPROFILE%` (or `%APPDATA%` for desktop apps). Project-scope paths are identical relative to the repo root.

Project scope is the right default when the skill is repo-specific (commit conventions, codebase quirks, internal CLIs). User scope is the right default when the skill is generally useful across all your work.

## Required User Gates

Before any write, ask the user the four destination questions. Skip a question only when the user has already answered it in this turn.

1. Which provider(s)? — one, several, or "all agents". Multi-select is allowed; each target may end up in a different folder.
2. Which scope for each provider? — user (global) vs project (per-repo) vs custom path. Ask per provider; do not assume one answer covers all.
3. If project scope: which project root? — confirm the absolute path; never guess from `cwd` alone if the workspace is ambiguous.
4. Install mode? — symlink (default for the Octocode CLI when the source is stable) or copy (portable snapshot when symlinks are unsuitable).

If the source has `scripts/`, install hooks, postinstall logic, or executable helpers, also ask: "Inspect scripts before install?" Default yes for third-party sources.

Recommended structured ask when the runtime provides one:

```text
Where do you want to install <skill-name>?

Provider(s):
- claude-code
- claude-desktop
- cursor
- codex
- opencode
- custom path

Scope (per provider):
- User (global) — applies everywhere
- Project (this repo) — applies only here; provide repo root
- Custom — provide absolute path

Install mode:
- Copy (recommended)
- Symlink (only for stable local sources)
```

## Conflict Handling

For every resolved destination, check for an existing folder before writing:

```bash
ls "<destination>/<skill-name>" 2>/dev/null
```

Ask per-destination when a conflict is found:

- `Overwrite` — replace the existing folder.
- `Skip` — leave existing in place, skip this destination only.
- `Rename` — install under a different `skill-name` (e.g. `<skill-name>-2`).
- `Diff` — show what differs before deciding.
- `Cancel` — abort the whole install.

Never silently overwrite. Never assume one conflict choice applies to all destinations.

## Install Checklist

Run only the items that fit the situation; do not force every step.

- Source resolved: folder contains a valid `SKILL.md` with `name` and `description` frontmatter.
- Safety scan: read `SKILL.md` and any `scripts/`, install hooks, or executable helpers; flag risky behavior to the user.
- Destinations confirmed: provider(s), scope(s), and final absolute path(s) shown to the user.
- Conflict check: per destination.
- Approval: explicit user confirmation to proceed.
- Fetch (remote sources only): download the source folder with the directory-mode tool of the active runtime; verify `SKILL.md` is present locally before copying.
- Write: copy (or symlink) into each destination. Prefer atomic copy when supported.
- Verify: `test -f "<destination>/<skill-name>/SKILL.md"` for every destination; report per-destination success or failure.
- Post-install hint (optional): tell the user how the active agent picks up new skills (restart, reload, or auto-scan).

## Symlink Notes

Use symlinks only when all of these hold:

- Source is a stable local path the user controls (excludes temp clones and remote downloads).
- User explicitly wants edits in source to reflect immediately.
- Destination runtime supports symlinked skill folders.

Otherwise, prefer copy.

## Recovery

- Destination does not exist yet: create the parent directory after explicit approval; do not auto-create deep custom paths.
- Permission denied: report the path and ask whether to try a different scope or a user-writable destination.
- Partial multi-target install: report which destinations succeeded and which failed; never roll back other targets without asking.
- Remote download incomplete: re-fetch once; if still incomplete, stop and report missing files.
- Frontmatter invalid: do not install; explain the gap.
- Unknown provider: treat as `custom path`; ask for an absolute destination and confirm the runtime scans it.
