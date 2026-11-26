Load when writing an MCP entry by hand, finding which file or variable sets a value, or choosing which GitHub credential Octocode uses.

# Configure Octocode

## Manual MCP configuration

For a client that accepts `mcpServers`, add the following entry and restart the client. Set the server working directory to the project when the client supports it.

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp"],
      "env": {"OCTOCODE_ENABLE_LOCAL": "true"}
    }
  }
}
```

`OCTOCODE_ENABLE_LOCAL` lists the tools that search and read code on this machine; leave it out for GitHub and package research only. With the standalone binary, set `command` to `octocode` and `args` to `["mcp"]`. If the client starts elsewhere, its server environment can set `WORKSPACE_ROOT` to the project’s absolute path for local reads. Project settings still come from the launch directory. [MCP setup and troubleshooting](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md)

## Configuration and folders

| Location | What it controls |
|---|---|
| Process environment or the MCP entry’s `env` | Settings for that process. |
| `<launch directory>/.octocode/` | Project `.env` and `.octocoderc` settings where workspace overrides are permitted. |
| Octocode home (normally `~/.octocode/`) | Shared `.env` and `.octocoderc`, saved logins, installed skills, and caches. `OCTOCODE_HOME` selects another directory. |

Highest priority first: process environment → project `.env` → home `.env` → project `.octocoderc` → home `.octocoderc` → default. Run `npx -y octocode config list` to inspect effective values and sources, or `npx -y octocode config home` to locate the active home. Removing an override with `config unset` lets the next applicable source supply the value. [Settings and workspace permissions](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md)

## GitHub credential selection

For the configured host, Octocode checks process tokens, project `.env`, home `.env`, the encrypted Octocode login, the OS credential store, then the host’s GitHub CLI login. Within a single environment layer the order is `GH_TOKEN`, then `GITHUB_TOKEN`. Public endpoints that allow anonymous access work without a login.

- **Device login** — Follow [Sign in to GitHub](../SKILL.md#sign-in-to-github). To remove the saved login for that host, use `npx -y octocode auth logout`.
- **CI or personal token** — Pass `GH_TOKEN` or `GITHUB_TOKEN` as a process secret. To save a personal token in the home `.env`, have the user enter it through `npx -y octocode config set GH_TOKEN --stdin`. This file stores the token in plain text.
- **GitHub Enterprise** — Set the Enterprise API root with `GITHUB_API_URL` in the process or home settings. Device login and refresh also require the host’s `OCTOCODE_GITHUB_CLIENT_ID`. [Authentication guide](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md)

If the CLI and agent use different accounts, compare their process environments, Octocode homes, and hostnames. An environment token takes precedence over a new device login.
