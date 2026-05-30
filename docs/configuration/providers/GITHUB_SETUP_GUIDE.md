# GitHub Setup Guide

> Complete guide for using Octocode MCP with GitHub — authentication, GitHub Enterprise, available tools, and troubleshooting.

---

## Quick Start

```bash
# Option A: Interactive login (recommended)
npx octocode-cli install
# -> Choose "Manage Auth", then "Login to GitHub"

# Option B: Direct CLI login
npx octocode-cli login

# Option C: Set a token manually
export GITHUB_TOKEN="ghp_xxxxxxxxxxxx"

# Start Octocode MCP — GitHub is the default provider
npx octocode-mcp
```

GitHub is the only supported provider.

---

## Authentication

Choose **one** of the following methods (listed from easiest to most manual):

### Option 1: Octocode CLI (Recommended)

The easiest way — handles secure OAuth login for you:

```bash
npx octocode-cli install
# -> Choose "Manage Auth", then "Login to GitHub"
```

You can also run `npx octocode-cli login` directly. Both paths open a browser window to authorize Octocode safely. The token is stored in Octocode's encrypted credential store (`~/.octocode/credentials.json`, encrypted with `~/.octocode/.key`).

### Option 2: GitHub CLI (`gh`)

If you already use the [GitHub CLI](https://cli.github.com/), Octocode automatically detects your credentials:

```bash
gh auth login
```

That's it — no additional setup needed.

### Option 3: Manual Token (Environment Variable)

Create a [GitHub Personal Access Token](https://github.com/settings/tokens) (Classic) with `repo` scope, then set it in one of these places:

**A. Shell Environment:**

```bash
export GITHUB_TOKEN="ghp_your_token_here"
```

**B. MCP Client Configuration (Cursor / VS Code / Claude Desktop):**

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

### Token Priority

When multiple tokens are available, Octocode uses the **first one found**:

| Priority | Source | How to set |
|----------|--------|------------|
| 1 (highest) | `OCTOCODE_TOKEN` env var | Octocode-specific token. Set in MCP client `"env"` block. |
| 2 | `GH_TOKEN` env var | Compatible with GitHub CLI. Set in MCP client `"env"` block. |
| 3 | `GITHUB_TOKEN` env var | Compatible with GitHub Actions. Set in MCP client `"env"` block. |
| 4 | `~/.octocode/credentials.json` | Stored by `npx octocode-cli install` during interactive auth (OAuth device flow). |
| 5 | `gh auth token` | Reads from GitHub CLI if installed and authenticated. |

**Minimum required scopes:** `repo`, `read:user`, `read:org`.

---

## GitHub Enterprise

For GitHub Enterprise Server, set the API URL alongside your token:

**Shell:**

```bash
export GITHUB_TOKEN="ghp_your_token_here"
export GITHUB_API_URL="https://github.mycompany.com/api/v3"
```

**MCP Client Configuration:**

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_your_token_here",
        "GITHUB_API_URL": "https://github.mycompany.com/api/v3"
      }
    }
  }
}
```

The default is `https://api.github.com` if `GITHUB_API_URL` is not set.

For available tools and detailed usage, see the [GitHub Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/GITHUB_TOOLS_REFERENCE.md). For all configuration options, see the [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md).

---

## Troubleshooting

### "No GitHub token found"

- Run `npx octocode-cli status` to check status, or `npx octocode-cli login` to authenticate directly.
- Ensure you have run `gh auth login` if using the GitHub CLI.
- Check if your environment variable is set: `echo $GITHUB_TOKEN`.

### 401 Unauthorized

- Verify your token is valid and hasn't expired.
- Check that the token has `repo` scope.
- For GitHub Enterprise: verify the API URL is correct.

### Token Expired

- Run `npx octocode-cli login` again to refresh it.
- Or run `gh auth refresh` if using the GitHub CLI.

### Switching Accounts

- Run `npx octocode-cli auth`, choose **Switch account**, then log in again. Octocode picks up the change immediately (no restart needed).

### Clone/Directory Tools Disabled

- Set `ENABLE_CLONE=true` in your MCP client `"env"` block (`ENABLE_LOCAL` is already `true` by default).

---

## See Also

- [Authentication Setup](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md) — Overview of all provider authentication
- [GitHub Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/GITHUB_TOOLS_REFERENCE.md) — Full tool documentation
- [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md) — All configuration options
- [Troubleshooting](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/TROUBLESHOOTING.md) — General troubleshooting guide

---
Created by Octocode MCP https://octocode.ai
