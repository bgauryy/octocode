# Skills Guide

Octocode can install any GitHub folder that contains a `SKILL.md` file, a GitHub skills library that contains multiple skill folders, or every official Octocode skill into the skill directories used by common AI coding clients.

Use this guide when you want a deterministic, scriptable install. The command is intentionally non-interactive: it defaults to `common` when no platform is supplied, accepts explicit destinations with `--platform`, refreshes the canonical source in `~/.octocode/skills`, and never prompts.

## Quick Start

```bash
# Shared cross-agent location
npx octocode skill --name octocode-research

# Pi's global agent skills directory
npx octocode skill --name octocode-research --platform pi

# GitHub Copilot / VS Code personal skills directory
npx octocode skill --name octocode-research --platform copilot

# Gemini CLI personal skills directory
npx octocode skill --name octocode-research --platform gemini

# Multiple clients, structured output for automation
npx octocode skill --add bgauryy/octocode/skills/octocode-research --platform cursor,codex --json

# Install every skill from a GitHub skills library path
npx octocode skill --add owner/repo/skills --platform common

# Install every current Octocode skill into Pi
npx octocode skill --install-all --platform pi
```

## Official Octocode Skills

The official catalog currently contains 8 user-installable skills. `octocode-research` is the recommended starting point for technical work because it covers investigation, implementation, review, refactor, architecture analysis, and evidence loops.

| Skill | Install | Use it when |
|-------|---------|-------------|
| `octocode` | `npx octocode skill --name octocode` | You need a quick Octocode-backed lookup. |
| `octocode-awareness` | `npx octocode skill --name octocode-awareness` | You need memory, file locks, handoffs, or verification records. |
| `octocode-brainstorming` | `npx octocode skill --name octocode-brainstorming` | You need to validate a fuzzy idea against evidence. |
| ⭐ `octocode-research` | `npx octocode skill --name octocode-research` | You need code research, code changes, review, refactor, or repeated proof loops. |
| `octocode-rfc-generator` | `npx octocode skill --name octocode-rfc-generator` | You need an RFC, migration plan, or implementation proposal. |
| `octocode-roast` | `npx octocode skill --name octocode-roast` | You want blunt but evidence-backed code critique. |
| `octocode-skills` | `npx octocode skill --name octocode-skills` | You are finding, installing, linting, or authoring Agent Skills. |
| `octocode-stats` | `npx octocode skill --name octocode-stats` | You want a local Octocode usage and savings dashboard. |

## Command Shape

```bash
npx octocode skill (--add <github-path> | --name <octocode-skill> | --install-all) [--platform common|cursor|claude|codex|opencode|pi|copilot|gemini|all] [--branch <ref>] [--mode symlink|copy|hybrid] [--force|--update] [--json]
```

| Option | Required | Use |
|--------|----------|-----|
| `--add <github-path>` | One source required | GitHub folder URL or `owner/repo/path` shorthand. If the path contains `SKILL.md`, Octocode installs that one skill. If it is a skills library such as `owner/repo/skills`, Octocode installs every direct child folder with a skill file. |
| `--name <octocode-skill>` | One source required | Named Octocode skill from the canonical Octocode skills catalog. Run `npx octocode skill --list` to browse names. |
| `--install-all` | One source required | Install every current official Octocode skill. Alias: `--all-skills`. |
| `--platform <list>` | No | Comma-separated `common`, `cursor`, `claude`, `codex`, `opencode`, `pi`, `copilot`, `gemini`, or `all`. Alias: `--target`. Default: `common`. |
| `--branch <ref>` | No | Branch, tag, or SHA when the input does not include one. |
| `--mode symlink|copy|hybrid` | No | `symlink` links clients to the refreshed source in `~/.octocode/skills`; `copy` duplicates the folder; `hybrid` copies for Claude and symlinks elsewhere. Default: `symlink`. |
| `--force` | No | Replace an existing installed skill folder/link. |
| `--update` | No | Alias for `--force`. The canonical source in `~/.octocode/skills` is refreshed even without this flag. |
| `--json` | No | Print machine-readable result data and errors. |

## GitHub Folder Inputs

Accepted forms:

```bash
npx octocode skill --add owner/repo/skills --platform common
npx octocode skill --add owner/repo/skills/my-skill --platform common
npx octocode skill --add owner/repo@main/skills/my-skill --platform cursor
npx octocode skill --add https://github.com/owner/repo/tree/main/skills/my-skill --platform claude
npx octocode skill --add https://github.com/owner/repo/blob/main/skills/my-skill/SKILL.md --platform codex
npx octocode skill --name octocode-research --platform pi
npx octocode skill --name octocode-research --platform copilot,gemini
```

If the URL points at `SKILL.md`, Octocode installs the containing folder. If the path points at a library such as `owner/repo/skills`, Octocode discovers direct child skill folders and installs them all.

## Platforms

| Platform | Default install location | Install with `npx octocode` |
|----------|--------------------------|-----------------------------|
| `common` | `~/.agents/skills` | `npx octocode skill --name octocode-research` |
| `cursor` | `~/.cursor/skills` | `npx octocode skill --name octocode-research --platform cursor` |
| `claude` | `~/.claude/skills` and `~/.claude-desktop/skills` | `npx octocode skill --name octocode-research --platform claude` |
| `codex` | `~/.agents/skills` | `npx octocode skill --name octocode-research --platform codex` |
| `opencode` | `~/.config/opencode/skills` | `npx octocode skill --name octocode-research --platform opencode` |
| `pi` | `~/.pi/agent/skills` | `npx octocode skill --name octocode-research --platform pi` |
| `copilot` | `~/.copilot/skills` | `npx octocode skill --name octocode-research --platform copilot` |
| `gemini` | `~/.gemini/skills` | `npx octocode skill --name octocode-research --platform gemini` |
| `all` | Common, Cursor, Claude, Codex, OpenCode, Pi, GitHub Copilot, and Gemini CLI | `npx octocode skill --name octocode-research --platform all` |

Platform-specific paths use the right home/AppData location on macOS, Linux, and Windows. Use `common` when you want one shared skill location that multiple agents can read.

## Copy vs Symlink

| Mode | Best for | Notes |
|------|----------|-------|
| `symlink` | Keeping all clients on one fetched source | Default. Octocode refreshes the source under `~/.octocode/skills/<skill>` and links each selected client directory to it. |
| `copy` | Durable installs per client | Each destination receives its own folder copied from `~/.octocode/skills/<skill>`. |
| `hybrid` | Claude plus symlink-friendly clients | Copies for Claude targets and symlinks everywhere else. |

The canonical source is overwritten on every install. Use `--force` when replacing an existing destination folder or link. Without `--force`, existing destinations are skipped, but symlinked clients still see the refreshed source.

## Agent-Safe Contract

`skill --add` is safe for automation because it does not ask follow-up questions. Missing or ambiguous input fails early.

| Case | Exit code |
|------|-----------|
| Missing source, invalid platform, invalid mode, or invalid GitHub path | `2` |
| GitHub path cannot be fetched, does not contain `SKILL.md`, or a GitHub library contains no skills | `3` |
| One or more destination installs fail | `1` |

JSON output includes:

```json
{
  "success": true,
  "skills": [
    {
      "name": "octocode-research",
      "displayName": "Octocode Research",
      "source": "https://github.com/bgauryy/octocode/tree/main/skills/octocode-research",
      "sourcePath": "<home>/.octocode/skills/octocode-research",
      "targets": [
        {
          "target": "agents",
          "path": "<home>/.agents/skills/octocode-research",
          "result": "installed"
        }
      ],
      "summary": {
        "installed": 1,
        "skipped": 0,
        "failed": 0
      }
    }
  ],
  "platforms": ["common"],
  "mode": "symlink",
  "summary": {
    "installed": 1,
    "skipped": 0,
    "failed": 0
  }
}
```

Human output always prints the selected mode, selected platforms, canonical source path, every platform destination path, and a final summary after an install attempt.

## Related Docs

- [Octocode CLI Guide](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md)
- [Skills Index](https://github.com/bgauryy/octocode/blob/main/skills/README.md)
- [Pi Setup Guide](https://github.com/bgauryy/octocode/blob/main/docs/PI/PI_SETUP_GUIDE.md)
