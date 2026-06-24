# CLI Docs

The Octocode CLI (`octocode`) runs quick research commands, executes all Octocode MCP tools directly from the terminal, manages GitHub auth, installs MCP client config, and reports status/cache state.

## Quick Start

```bash
npx octocode --help            # all commands
npx octocode tools             # list tools
npx octocode tools <name> --scheme  # schema for one tool
npx octocode status --json     # auth/token/cache/MCP status
npx octocode install --ide cursor  # install MCP for an IDE
npx octocode skill --add bgauryy/octocode-mcp/skills/octocode-engineer --platform common
```

`skill --add` is the agent-safe skill installer. It requires every destination as a flag and never prompts, so automation can use `--platform common|cursor|claude|codex|all`, `--mode copy|symlink`, `--force`, and `--json` deterministically.

## Docs

| Doc | Purpose |
|-----|---------|
| [REFERENCE.md](https://github.com/bgauryy/octocode/blob/main/docs/cli/REFERENCE.md) | All commands, flags, options, exit codes |
| [SKILLS_GUIDE.md](https://github.com/bgauryy/octocode/blob/main/docs/SKILLS_GUIDE.md) | Install GitHub Agent Skill folders into supported agent clients |
| [GITHUB_API_BENCHMARK.md](https://github.com/bgauryy/octocode/blob/main/docs/cli/GITHUB_API_BENCHMARK.md) | GitHub API benchmark notes |

## Supported IDE Clients

`cursor`, `claude-desktop`, `claude-code`, `windsurf`, `zed`, `vscode-cline`, `vscode-roo`, `vscode-continue`, `opencode`, `trae`, `antigravity`, `codex`, `gemini-cli`, `goose`, `kiro`

[← docs index](https://github.com/bgauryy/octocode/blob/main/docs/README.md)
