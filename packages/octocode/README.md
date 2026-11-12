# Octocode CLI

Search local source, read GitHub code and history, and resolve package versions from your terminal. The npm launcher requires Node.js 22 or later and runs the native Octocode binary for your platform.

## First read

In a project with a README, run:

```bash
npx -y octocode localFetch '{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}'
```

The result contains the first source line. Add `--json` for an indexed result envelope. A tool accepts JSON inline, from `--input FILE` or from stdin with `--input -`.

| Goal | Command |
|---|---|
| List available tools | `npx -y octocode schema` |
| See a tool's operations and example inputs | `npx -y octocode schema localSearch --view variants` |
| Check which GitHub account is in use | `npx -y octocode auth status` |
| Sign in for code search or private repositories | `npx -y octocode auth login` |
| Inspect settings, sources and defaults | `npx -y octocode config list` |
| Connect an MCP client | `npx -y octocode install` |
| Browse bundled agent workflows | `npx -y octocode skill list` |

The CLI and MCP accept the same tool inputs. If a result has more pages, run its emitted continuation unchanged. [CLI output and exit codes](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md) · [Local, GitHub and package tools](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md) · [Settings and opt-ins](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) · [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md)

## Development

From the repository root, run `yarn workspace octocode build` and `yarn workspace octocode test`. [Launcher architecture](https://github.com/bgauryy/octocode/blob/main/packages/octocode/ARCHITECTURE.md)

## License

MIT
