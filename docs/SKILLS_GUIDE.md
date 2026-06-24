# Skills Guide

Octocode can install any GitHub folder that contains a `SKILL.md` file into the skill directories used by common AI coding clients.

Use this guide when you want a deterministic, scriptable install. The command is intentionally non-interactive: agents must pass every destination with `--platform`, and the CLI never prompts.

## Quick Start

```bash
# Shared cross-agent location
npx octocode skill --add bgauryy/octocode-mcp/skills/octocode-engineer --platform common

# Multiple clients, structured output for automation
npx octocode skill --add bgauryy/octocode-mcp/skills/octocode-engineer --platform cursor,codex --mode copy --json

# Keep one fetched source and link every supported client to it
npx octocode skill --add https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-engineer --platform all --mode symlink --force
```

## Command Shape

```bash
octocode skill --add <github-folder> --platform common|cursor|claude|codex|all [--branch <ref>] [--mode copy|symlink] [--force] [--json]
```

| Option | Required | Use |
|--------|----------|-----|
| `--add <github-folder>` | Yes | GitHub folder URL or `owner/repo/path` shorthand. The folder must contain `SKILL.md`. |
| `--platform <list>` | Yes | Comma-separated `common`, `cursor`, `claude`, `codex`, or `all`. Alias: `--target`. |
| `--branch <ref>` | No | Branch, tag, or SHA when the input does not include one. |
| `--mode copy|symlink` | No | `copy` duplicates the fetched folder; `symlink` links clients to Octocode's source cache. Default: `copy`. |
| `--force` | No | Replace an existing installed skill folder/link. |
| `--json` | No | Print machine-readable result data and errors. |

## GitHub Folder Inputs

Accepted forms:

```bash
octocode skill --add owner/repo/skills/my-skill --platform common
octocode skill --add owner/repo@main/skills/my-skill --platform cursor
octocode skill --add https://github.com/owner/repo/tree/main/skills/my-skill --platform claude
octocode skill --add https://github.com/owner/repo/blob/main/skills/my-skill/SKILL.md --platform codex
```

If the URL points at `SKILL.md`, Octocode installs the containing folder. Before installing, it fetches and validates that `SKILL.md` exists.

## Platforms

| Platform | Installs to |
|----------|-------------|
| `common` | `~/.agents/skills` |
| `cursor` | Cursor's skills directory |
| `claude` | Claude Code and Claude Desktop skill directories |
| `codex` | Codex's skills directory |
| `all` | Common, Cursor, Claude, and Codex |

Platform-specific paths use the right home/AppData location on macOS, Linux, and Windows. Use `common` when you want one shared skill location that multiple agents can read.

## Copy vs Symlink

| Mode | Best for | Notes |
|------|----------|-------|
| `copy` | Durable installs per client | Each destination receives its own folder. |
| `symlink` | Keeping all clients on one fetched source | Octocode keeps the source under `<octocode-home>/skill-sources/` and links each selected client directory to it. |

Use `--force` when replacing an existing destination. Without `--force`, existing skills are skipped instead of overwritten.

## Agent-Safe Contract

`skill --add` is safe for automation because it does not ask follow-up questions. Missing or ambiguous input fails early.

| Case | Exit code |
|------|-----------|
| Missing `--add`, missing `--platform`, invalid platform, invalid mode, or invalid GitHub folder | `2` |
| GitHub folder cannot be fetched or does not contain `SKILL.md` | `3` |
| One or more destination installs fail | `1` |

JSON output includes:

```json
{
  "success": true,
  "skill": "octocode-engineer",
  "source": "https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-engineer",
  "cachePath": "<octocode-home>/skill-sources/github-bgauryy-octocode-mcp-main-skills-octocode-engineer/octocode-engineer",
  "platforms": ["common"],
  "targets": [
    {
      "target": "agents",
      "path": "<home>/.agents/skills/octocode-engineer"
    }
  ],
  "mode": "copy",
  "installed": 1,
  "skipped": 0,
  "failed": 0
}
```

## Related Docs

- [CLI Reference](https://github.com/bgauryy/octocode/blob/main/docs/cli/REFERENCE.md)
- [Skills Index](https://github.com/bgauryy/octocode/blob/main/skills/README.md)
- [Pi Setup Guide](https://github.com/bgauryy/octocode/blob/main/docs/PI/PI_SETUP_GUIDE.md)
