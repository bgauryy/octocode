# Skill Sync

Load when symlinking a local skill into vendor skill dirs (Claude, Cursor, Codex, `.agents`, …). Why: one source path, many hosts — never write without human approval.

## Human gate

Default is **dry-run** (plan only). Agents must show the plan and wait for explicit human OK before `--approve`.

```bash
node scripts/skill-sync.mjs <skill-dir> --platforms top
node scripts/skill-sync.mjs <skill-dir> --platforms top --approve
node scripts/skill-sync.mjs <skill-dir> --platforms claude,cursor --approve --force
node scripts/skill-sync.mjs --list-vendors
```

`--force` replaces conflicts and **requires** `--approve`. No interactive prompts.

## Top vendors (`--platforms top`)

| Id | User path |
|----|-----------|
| `claude` | `~/.claude/skills` |
| `cursor` | `~/.cursor/skills` |
| `agents` | `~/.agents/skills` |
| `codex-native` | `~/.codex/skills` |

`all` adds: `claude-desktop`, `codex` (→ `~/.agents/skills`), `opencode`, `pi`, `copilot`, `gemini`. Full map: `--list-vendors` or `references/install-destinations.md`.

## When to use

- Stable local skill source the user controls (dev dogfood / live edits).
- User asked to sync/symlink to Claude, Cursor, Codex, or `.agents`.
- Prefer Octocode CLI copy/symlink when installing published skills: `npx octocode skill --add …`.

Never symlink a temp fetch — use `references/fetch-remote.md` + copy instead.

Next: when choosing scopes load `references/install-destinations.md`; when gating install load `references/install-gates.md`.
