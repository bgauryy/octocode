# Install Destinations

Load when choosing where a skill lands — after `install-gates.md`. Why: wrong scope = skill invisible or pollutes every project.

| Provider | User (global) | Project (per-repository) |
|----------|---------------|--------------------|
| claude (`claude-desktop` alias) | `~/.claude/skills/` | `<repo>/.claude/skills/` |
| cursor | `~/.cursor/skills/` | `<repo>/.cursor/skills/` |
| codex / shared agents (`agents`, `shared`, `common`, `codex-native` aliases) | `~/.agents/skills/` | `<repo>/.agents/skills/` |
| opencode | `~/.config/opencode/skills/` | `<repo>/.opencode/skills/` |
| pi | `~/.pi/agent/skills/` | `<repo>/.pi/skills/` |
| copilot | `~/.copilot/skills/` | `<repo>/.github/skills/` |
| gemini | `~/.gemini/skills/` | `<repo>/.gemini/skills/` |
| other | path the runtime scans | in-repository path you confirm |

Symlink sync to these dirs: `scripts/skill-sync.mjs` (`references/skill-sync.md`) — dry-run, then human `--approve`.

Windows: `~` resolves to the account profile. Custom override = supplied absolute path.

## Scope defaults

- Project — repository-specific (commit conventions, internal CLIs, codebase quirks).
- User — generally useful across all work.

Unknown provider → treat as custom path; confirm the runtime scans it.

## Recovery (destination)

- Missing parent dir: create after approval; don't auto-create deep custom trees.
- Permission denied: report path; offer different scope.
- Partial multi-target: report per destination; don't roll back others without asking.
- Invalid frontmatter: do not install.

Next: when syncing vendors load `references/skill-sync.md`; when source is remote load `references/fetch-remote.md`; if the skill bundles hooks load `references/hooks.md`.
