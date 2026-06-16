# Authentication Setup

GitHub is the only supported Octocode provider. This page covers login, token priority, GitHub Enterprise, and clone-tool auth.

## Quick Start

```bash
# Recommended OAuth login
npx octocode-cli login

# Or use GitHub CLI credentials
gh auth login

# Or provide a token directly
export GITHUB_TOKEN="ghp_your_token_here"
```

Start the MCP server after one authentication method is configured:

```bash
npx octocode-mcp
```

## Authentication Methods

| Method | Use when | Command or setting |
|--------|----------|--------------------|
| Octocode CLI OAuth | You want the simplest setup | `npx octocode-cli login` |
| GitHub CLI | You already use `gh` locally | `gh auth login` |
| Environment token | You are configuring an MCP client, CI, or Enterprise host | `OCTOCODE_TOKEN`, `GH_TOKEN`, or `GITHUB_TOKEN` |

Octocode CLI stores OAuth credentials encrypted in `~/.octocode/credentials.json` with the key in `~/.octocode/.key`.

For manual tokens, create a GitHub token with access to the repositories you need. Private repository and organization access usually needs `repo`, `read:user`, and `read:org`.

## MCP Client Example

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_your_token_here"
      }
    }
  }
}
```

## Token Priority

When multiple tokens are available, Octocode uses the first source found:

| Priority | Source |
|----------|--------|
| 1 | `OCTOCODE_TOKEN` environment variable |
| 2 | `GH_TOKEN` environment variable |
| 3 | `GITHUB_TOKEN` environment variable |
| 4 | Octocode encrypted credentials |
| 5 | `gh auth token` |

Environment tokens are user-managed. Octocode credentials can refresh when a refresh token is available. GitHub CLI token refresh is handled by `gh`.

## GitHub Enterprise

Set `GITHUB_API_URL` to the Enterprise API endpoint and provide a token for that host:

```bash
export GITHUB_TOKEN="ghp_your_enterprise_token"
export GITHUB_API_URL="https://github.mycompany.com/api/v3"
```

The default API URL is `https://api.github.com`.

## Clone And Directory Tools

Remote GitHub tools work with token auth alone. Clone-backed workflows also need local tools and clone access enabled:

```bash
export ENABLE_LOCAL=true
export ENABLE_CLONE=true
```

This enables `ghCloneRepo` and `ghGetFileContent` with `type: "directory"`.

## Checks

| Symptom | Check |
|---------|-------|
| No token found | Run `npx octocode-cli status`, then `npx octocode-cli login`, or set an environment token. |
| 401 Unauthorized | Check token validity, scopes, and the selected GitHub host. |
| Enterprise requests hit github.com | Set `GITHUB_API_URL` to the Enterprise API URL. |
| Clone tools unavailable | Set both `ENABLE_LOCAL=true` and `ENABLE_CLONE=true`. |
| Wrong account | Run `npx octocode-cli auth` or update the active `gh` account. |

## See Also

- [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md)
- [GitHub Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/GITHUB_TOOLS_REFERENCE.md)
- [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md)
- [Credentials Architecture](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/architecture/CREDENTIALS_ARCHITECTURE.md)
