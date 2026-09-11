# Skill Sync

Load when symlinking a local skill into vendor skill dirs (Claude, Cursor, Codex, `.agents`, …). Why: one source path, many hosts — never write without human approval.

## Human gate

Default is **dry-run** (plan only). Inspect the plan before `--approve`. Existing authorization can cover its source, destinations, and conflict policy; ask only when the plan requires authority that has not been granted.

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
| `codex` | `~/.agents/skills` |

`all` adds the distinct `opencode`, `pi`, `copilot`, and `gemini` destinations. `agents`, `shared`, `common`, and `codex-native` alias `codex`; `claude-desktop` aliases `claude`. The standalone script mirrors the shared installer registry and CI contract-tests that mirror because an installed skill cannot import workspace packages. Full map: `--list-vendors` or `references/install-destinations.md`.

## When to use

- Stable local skill source that you control (development dogfood or live edits).
- User asked to sync/symlink to Claude, Cursor, Codex, or `.agents`.
- Prefer Octocode CLI copy/symlink when installing published skills: `npx -y octocode skill install --add …`.

Never symlink a temp fetch — use `references/fetch-remote.md` + copy instead.

Next: when choosing scopes load `references/install-destinations.md`; when gating install load `references/install-gates.md`.
