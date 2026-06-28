# CLI Docs

The Octocode CLI (`octocode`) runs quick research commands, executes all Octocode MCP tools directly from the terminal, manages GitHub auth, installs MCP client config, and reports status/cache state.

## Quick Start

```bash
npx octocode --help            # all commands
npx octocode tools             # list tools
npx octocode tools <name> --scheme  # schema for one tool
npx octocode status --json     # auth/token/cache/MCP status
npx octocode install --ide cursor  # install Octocode MCP for an IDE
npx octocode skill --name octocode-engineer --platform pi
for skill in octocode octocode-awareness octocode-brainstorming octocode-engineer octocode-loop octocode-research octocode-rfc-generator octocode-roast octocode-skills octocode-stats; do npx octocode skill --name "$skill" --platform pi --mode copy --update; done
```

`skill` is the agent-safe skill installer. It never prompts, so automation can use `--name <octocode-skill>` or `--add <github-folder>`, `--platform common|cursor|claude|codex|opencode|pi|all`, `--mode copy|symlink|hybrid`, `--force|--update`, and `--json` deterministically.

## Docs

| Doc | Purpose |
|-----|---------|
| [REFERENCE.md](https://github.com/bgauryy/octocode/blob/main/docs/cli/REFERENCE.md) | All commands, flags, options, exit codes |
| [SKILLS_GUIDE.md](https://github.com/bgauryy/octocode/blob/main/docs/SKILLS_GUIDE.md) | Install GitHub Agent Skill folders into supported agent clients |
| [GITHUB_API_BENCHMARK.md](https://github.com/bgauryy/octocode/blob/main/docs/cli/GITHUB_API_BENCHMARK.md) | GitHub API benchmark notes |

## Supported IDE Clients

`cursor`, `claude-desktop`, `claude-code`, `windsurf`, `zed`, `vscode-cline`, `vscode-roo`, `vscode-continue`, `opencode`, `trae`, `antigravity`, `codex`, `gemini-cli`, `goose`, `kiro`

[← docs index](https://github.com/bgauryy/octocode/blob/main/docs/README.md)
