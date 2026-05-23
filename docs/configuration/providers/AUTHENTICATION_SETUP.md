# Authentication Setup

> How to authenticate Octocode MCP with GitHub, GitLab, or Bitbucket.

## Provider Priority

When multiple provider tokens are set, Octocode selects the active provider in this order:

```
GitLab (highest) → Bitbucket → GitHub (default)
```

- If `GITLAB_TOKEN` is set → **GitLab** is active (regardless of other tokens).
- If `BITBUCKET_TOKEN` is set (and no GitLab token) → **Bitbucket** is active.
- Otherwise → **GitHub** is the default.

To switch providers, change which token environment variables are set and restart the MCP server.

## Provider Setup Guides

Each guide covers step-by-step authentication, token creation, provider-specific troubleshooting, and known limitations:

| Provider | Guide |
|----------|-------|
| **GitHub** | [GitHub Setup Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/GITHUB_SETUP_GUIDE.md) |
| **GitLab** | [GitLab Setup Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/GITLAB_SETUP_GUIDE.md) |
| **Bitbucket** | [Bitbucket Setup Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/BITBUCKET_SETUP_GUIDE.md) |

For all configuration options (env vars, `.octocoderc`, tool filtering, network tuning), see the [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md).

For available tools and usage, see the [GitHub, GitLab & Bitbucket Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/GITHUB_GITLAB_TOOLS_REFERENCE.md).
