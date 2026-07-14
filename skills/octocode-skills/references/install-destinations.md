# Install Destinations

Load when choosing where a skill lands — after `install-gates.md`. Why: wrong scope = skill invisible or pollutes every project.

| Provider | User (global) | Project (per-repo) |
|----------|---------------|--------------------|
| shared agents | `~/.agents/skills/` | `<repo>/.agents/skills/` |
| claude-code | `~/.claude/skills/` | `<repo>/.claude/skills/` |
| claude-desktop | `~/.claude-desktop/skills/` | n/a |
| cursor | `~/.cursor/skills/` | `<repo>/.cursor/skills/` |
| codex | `~/.agents/skills/` | `<repo>/.agents/skills/` |
| codex-native | `~/.codex/skills/` | n/a |
| opencode | `~/.config/opencode/skills/` | `<repo>/.opencode/skills/` |
| pi | `~/.pi/agent/skills/` | `<repo>/.pi/skills/` |
| copilot | `~/.copilot/skills/` | `<repo>/.github/skills/` |
| gemini | `~/.gemini/skills/` or `~/.agents/skills/` | matching project path |
| other | path the runtime scans | in-repo path user confirms |

Symlink sync to these dirs: `scripts/skill-sync.mjs` (`references/skill-sync.md`) — dry-run, then human `--approve`.

Windows: `~` → `%USERPROFILE%` (or `%APPDATA%` for desktop apps). Custom override = user-supplied absolute path.

## Scope defaults

- Project — repo-specific (commit conventions, internal CLIs, codebase quirks).
- User — generally useful across all work.

Unknown provider → treat as custom path; confirm the runtime scans it.

## Recovery (destination)

- Missing parent dir: create after approval; don't auto-create deep custom trees.
- Permission denied: report path; offer different scope.
- Partial multi-target: report per destination; don't roll back others without asking.
- Invalid frontmatter: do not install.

Next: when syncing vendors load `references/skill-sync.md`; when source is remote load `references/fetch-remote.md`; if the skill bundles hooks load `references/hooks.md`.
