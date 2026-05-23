# Configuration Reference

> Complete guide to configuring Octocode MCP — where to set options, what each option does, and how they interact.

## Two Ways to Configure

Octocode reads configuration from **two sources**. You can use either or both:

### 1. Environment Variables (in your MCP client settings)

Your MCP client (Cursor, VS Code, Claude Desktop, etc.) has a settings file where you declare MCP servers. Environment variables go in the `"env"` block of your server config.

The JSON structure is the same across all clients — only the file location differs:

| Client | Config file |
|--------|------------|
| Cursor | `~/.cursor/mcp.json` |
| VS Code | `.vscode/mcp.json` |
| Claude Desktop | `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` |

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_xxxxxxxxxxxx",
        "ENABLE_LOCAL": "true"
      }
    }
  }
}
```

Environment variables are ideal for per-project or per-session settings — especially auth tokens and feature flags.

### 2. The `.octocoderc` Config File (persistent defaults)

A JSON file stored on your machine that applies to **all** sessions. Supports comments and trailing commas. Ideal for machine-wide defaults that don't change between projects (API URLs, network tuning, tool preferences).

| Platform | Path |
|----------|------|
| macOS / Linux | `~/.octocode/.octocoderc` |
| Windows | `%USERPROFILE%\.octocode\.octocoderc` |

**Quick setup (macOS / Linux):**

```bash
mkdir -p ~/.octocode
cat > ~/.octocode/.octocoderc << 'EOF'
{
  "github": {
    "apiUrl": "https://api.github.com"
  },
  "local": {
    "enabled": true
  }
}
EOF
```

**Quick setup (Windows PowerShell):**

```powershell
New-Item -ItemType Directory -Force -Path "$env:USERPROFILE\.octocode"
@'
{
  "github": {
    "apiUrl": "https://api.github.com"
  },
  "local": {
    "enabled": true
  }
}
'@ | Out-File -Encoding utf8 "$env:USERPROFILE\.octocode\.octocoderc"
```

**Complete schema:**

```jsonc
{
  "version": 1,

  "github": {
    "apiUrl": "https://api.github.com"       // GitHub API endpoint
  },

  "gitlab": {
    "host": "https://gitlab.com"             // GitLab instance URL
  },

  "bitbucket": {
    "host": "https://api.bitbucket.org/2.0"  // Bitbucket API endpoint
  },

  "local": {
    "enabled": true,                         // Enable local filesystem + LSP tools
    "enableClone": false,                    // Enable repo cloning (requires enabled=true)
    "workspaceRoot": "/path/to/workspace",   // Root for local operations
    "allowedPaths": []                       // Restrict to these paths (empty = all)
  },

  "tools": {
    "enabled": null,                         // Strict whitelist (null = all tools)
    "enableAdditional": null,                // Add extra tools
    "disabled": null,                        // Remove specific tools
    "disablePrompts": false                  // Disable MCP prompts registration
  },

  "network": {
    "timeout": 30000,                        // Request timeout in ms (5000–300000)
    "maxRetries": 3                          // Retry attempts (0–10)
  },

  "telemetry": {
    "logging": true                          // Telemetry
  },

  "lsp": {
    "configPath": null                       // Custom LSP config file path
  },

  "output": {
    "format": "yaml",                        // Response format: "yaml" (default) or "json"
    "pagination": {
      "defaultCharLength": 8000             // Default output page budget for auto-pagination
    }
  }
}
```

**Validation:** The file is validated on load. Invalid values don't prevent startup — defaults are used instead. URLs must start with `http://` or `https://`. Numbers are clamped to valid range. Unknown keys are ignored with a warning. Parse errors skip the entire file with a warning.

### Resolution Order

When both sources set the same option, environment variables always win:

```
Environment Variable  >  .octocoderc File  >  Built-in Default
     (highest)             (fallback)          (last resort)
```

This means you can set sensible defaults in `.octocoderc` and override specific values per-project in your MCP client config.

---

## Authentication

Auth tokens are **environment-variable only** — never store tokens in `.octocoderc`.

Octocode supports three providers. The active provider is selected based on which tokens are set:

```
GitLab (highest priority) → Bitbucket → GitHub (default)
```

| Provider | Setup Guide | Key Variables |
|----------|-------------|---------------|
| **GitHub** | [GitHub Setup Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/GITHUB_SETUP_GUIDE.md) | `GITHUB_TOKEN`, `GH_TOKEN`, `OCTOCODE_TOKEN` |
| **GitLab** | [GitLab Setup Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/GITLAB_SETUP_GUIDE.md) | `GITLAB_TOKEN`, `GL_TOKEN`, `GITLAB_HOST` |
| **Bitbucket** | [Bitbucket Setup Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/BITBUCKET_SETUP_GUIDE.md) | `BITBUCKET_TOKEN`, `BB_TOKEN`, `BITBUCKET_USERNAME` |

For full authentication details (token creation, auth modes, troubleshooting), see the [Authentication Setup](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md) overview or the individual provider guides above.

---

## Octocode Home Directory

Octocode stores local state under `~/.octocode` by default (credentials, config, session identity, usage stats, clone cache, logs).

You can override the root directory with `OCTOCODE_HOME`:

```bash
export OCTOCODE_HOME=/data/octocode
```

When set, these paths move under the new root:

- `.octocoderc` -> `${OCTOCODE_HOME}/.octocoderc`
- credentials -> `${OCTOCODE_HOME}/credentials.json`
- session -> `${OCTOCODE_HOME}/session.json`
- usage stats -> `${OCTOCODE_HOME}/stats.json`
- clone cache -> `${OCTOCODE_HOME}/repos/`
- logs -> `${OCTOCODE_HOME}/logs/`
- LSP user config -> `${OCTOCODE_HOME}/lsp-servers.json`

---

## Usage Stats and Savings

Octocode automatically tracks local usage counters in `stats.json` under the Octocode home directory. This file is separate from `session.json`: the session file stores identity and timestamps, while `stats.json` stores cumulative counters that can be shown to users.

Default path:

```bash
~/.octocode/stats.json
```

With `OCTOCODE_HOME`:

```bash
${OCTOCODE_HOME}/stats.json
```

Example:

```json
{
  "version": 1,
  "stats": {
    "toolCalls": 142,
    "promptCalls": 3,
    "errors": 2,
    "rateLimits": 3,
    "rateLimitsByProvider": {
      "github": 1,
      "gitlab": 1,
      "bitbucket": 1
    },
    "charsSavedByTool": {
      "githubSearchCode": {
        "rawChars": 120000,
        "responseChars": 18000,
        "savedChars": 102000,
        "calls": 6
      }
    },
    "githubCacheHits": {
      "hits": {
        "gh-api-code": 12,
        "gh-api-prs": 3
      },
      "rateLimits": 1
    },
    "packageRegistryFailures": {
      "npm": 2
    },
    "totalUsage": {
      "toolCalls": 142,
      "promptCalls": 3,
      "errors": 2,
      "rateLimits": 3,
      "rateLimitsByProvider": {
        "github": 1,
        "gitlab": 1,
        "bitbucket": 1
      },
      "rawChars": 120000,
      "responseChars": 18000,
      "savedChars": 102000,
      "charSavingsCalls": 6,
      "githubCacheHits": 15,
      "githubCacheRateLimits": 1,
      "packageRegistryFailures": 2,
      "packageRegistryFailuresByRegistry": {
        "npm": 2
      }
    }
  }
}
```

Tracked stats:

| Field | Description |
|-------|-------------|
| `toolCalls` | Total MCP tool calls handled by Octocode. |
| `promptCalls` | Total MCP prompt calls handled by Octocode. |
| `errors` | Total logged Octocode errors. |
| `rateLimits` | Total provider API rate-limit encounters. Package registry failures are tracked separately. |
| `rateLimitsByProvider` | Provider API rate-limit encounters by provider, including `github`, `gitlab`, and `bitbucket`. |
| `charsSavedByTool` | Per-tool source/raw character count, final returned character count, saved character count, and call count. |
| `githubCacheHits.hits` | Per GitHub cache bucket hit counts, such as `gh-api-code`, `gh-api-prs`, and `gh-repo-structure-api`. |
| `githubCacheHits.rateLimits` | GitHub-specific rate-limit encounters stored alongside GitHub cache stats, including API errors and Octokit retry-throttle hits from any GitHub-backed tool. |
| `packageRegistryFailures` | Package registry HTTP failures by registry, such as `npm` and `pypi`. These are not counted as provider API rate limits. |
| `totalUsage` | Derived aggregate totals for display: overall counters, provider rate-limit breakdown, total raw characters, total returned characters, total saved characters, char-savings call count, total GitHub cache hits, GitHub cache rate-limit count, and package-registry failure totals. |

For every registered tool, Octocode records source/raw characters before Octocode-specific trimming, filtering, verbosity reduction, and bulk response pagination when that source size is available. The returned character count is the final MCP tool text response. Bulk and parallel calls are aggregated once per tool invocation, including mixed success/error query results.

`totalUsage` is recalculated whenever stats are read or written, so dashboards can read a single object without separately summing `charsSavedByTool` and `githubCacheHits`. The per-tool and per-cache counters remain the source of truth for detailed breakdowns.

These counters are written locally regardless of remote telemetry logging. Setting `LOG=false` disables remote telemetry, but it does not disable local `stats.json` updates.

For implementation details, see [Session Persistence](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/architecture/SESSION_PERSISTENCE.md).

---

## All Configuration Options

| # | Env Variable | `.octocoderc` Field | Type | Default | Description |
|---|---|---|---|---|---|
| | **Provider API Endpoints** | | | | |
| 1 | `GITHUB_API_URL` | `github.apiUrl` | string | `https://api.github.com` | GitHub API endpoint. Change for GitHub Enterprise. |
| 2 | `GITLAB_HOST` | `gitlab.host` | string | `https://gitlab.com` | GitLab instance URL. Use env var for reliability. |
| 3 | `BITBUCKET_HOST` | `bitbucket.host` | string | `https://api.bitbucket.org/2.0` | Bitbucket Cloud API endpoint. |
| | **Local Tools** | | | | |
| 4 | `ENABLE_LOCAL` | `local.enabled` | boolean | `true` | Enable local filesystem + LSP tools. |
| 5 | `ENABLE_CLONE` | `local.enableClone` | boolean | `false` | Enable repo cloning (`githubCloneRepo`) and directory fetch. **Requires `ENABLE_LOCAL=true`.** |
| 6 | `WORKSPACE_ROOT` | `local.workspaceRoot` | string | `process.cwd()` | Root directory for local tool operations. |
| 7 | `ALLOWED_PATHS` | `local.allowedPaths` | list | `[]` (all) | Restrict local tools to these directory paths. Empty = unrestricted. |
| | **Tool Filtering** | | | | |
| 8 | `TOOLS_TO_RUN` | `tools.enabled` | list | `null` (all) | **Strict whitelist.** When set, only these tools are available. Overrides #9 and #10. |
| 9 | `ENABLE_TOOLS` | `tools.enableAdditional` | list | `null` | Add extra tools to the default set. Ignored when #8 is set. |
| 10 | `DISABLE_TOOLS` | `tools.disabled` | list | `null` | Remove tools from the default set. Ignored when #8 is set. |
| 11 | `DISABLE_PROMPTS` | `tools.disablePrompts` | boolean | `false` | Disable MCP prompts registration (slash commands / agent instructions). |
| | **Network** | | | | |
| 12 | `REQUEST_TIMEOUT` | `network.timeout` | number | `30000` | Request timeout in ms. Range: 5,000–300,000. Values outside range are clamped. |
| 13 | `MAX_RETRIES` | `network.maxRetries` | number | `3` | Max retry attempts. Range: 0–10. Clamped. |
| | **Telemetry** | | | | |
| 14 | `LOG` | `telemetry.logging` | logging | `true` | Telemetry. Disabled with `false`/`0`. |
| | **LSP** | | | | |
| 15 | `OCTOCODE_LSP_CONFIG` | `lsp.configPath` | string | `null` | Custom LSP config file path. Auto-detects `.octocode/lsp-servers.json` when unset. Requires `ENABLE_LOCAL=true`. |
| | **Output** | | | | |
| 16 | `OCTOCODE_OUTPUT_FORMAT` | `output.format` | string | `yaml` | Response serialization format. `yaml` (default, token-efficient) or `json` (raw JSON). |
| 17 | `OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH` | `output.pagination.defaultCharLength` | number | `8000` | Default output page budget for automatic pagination. Used by all tools unless a request overrides it with `charLength` or `responseCharLength`. |
| | **Authentication** (env only) | | | | |
| 18 | `OCTOCODE_TOKEN` | — | string | — | GitHub token (priority 1). |
| 19 | `GH_TOKEN` | — | string | — | GitHub CLI token (priority 2). |
| 20 | `GITHUB_TOKEN` | — | string | — | GitHub Actions token (priority 3). |
| 21 | `GITLAB_TOKEN` | — | string | — | GitLab personal access token (priority 1). Setting this activates GitLab mode. |
| 22 | `GL_TOKEN` | — | string | — | GitLab token fallback (priority 2). Setting this activates GitLab mode. |
| 23 | `BITBUCKET_TOKEN` | — | string | — | Bitbucket app password or OAuth token (priority 1). Setting this activates Bitbucket mode (when no GitLab token is set). |
| 23 | `BB_TOKEN` | — | string | — | Bitbucket token fallback (priority 2). |
| 24 | `BITBUCKET_USERNAME` | — | string | — | Bitbucket username. Set for Basic auth (app passwords); omit for Bearer auth (OAuth). |
| | **Advanced** (env only) | | | | |
| 25 | `OCTOCODE_BULK_QUERY_TIMEOUT_MS` | — | number | `60000` | Timeout for bulk/multi-query tool calls (ms). |
| 26 | `OCTOCODE_COMMAND_CHECK_TIMEOUT_MS` | — | number | `5000` | Timeout for checking system command availability (ms). |
| 27 | `OCTOCODE_CACHE_TTL_MS` | — | number | `86400000` | Cache TTL for cloned repos (ms). Default is 24 hours. Must be a positive integer. |
| 28 | `OCTOCODE_HOME` | — | string | `~/.octocode` | Override Octocode home directory for all local state (config, credentials, repos, logs, session, stats). |
| 29 | `OCTOCODE_MAX_CACHE_SIZE` | — | number | `2147483648` | Maximum clone cache disk usage in bytes (default 2 GB). Evicts oldest clones when exceeded. |
| 30 | `OCTOCODE_MAX_CLONES` | — | number | `50` | Maximum number of cached clones. Evicts oldest clones when exceeded. |

**Type parsing (all values are case-insensitive, whitespace is trimmed):**

| Type | Accepted values | Invalid input |
|------|----------------|---------------|
| **boolean** | `true`, `1` = on; `false`, `0` = off | Ignored (default used) |
| **logging** | `false`, `0` = off; everything else = on | Treated as on |
| **number** | Integer string, clamped to valid range | Ignored (default used) |
| **list** | Comma-separated (e.g., `"a,b,c"`) | — |
| **string** | Any value | — |

### Notes

- **Tool filtering:** `TOOLS_TO_RUN` is a strict whitelist that overrides both `ENABLE_TOOLS` and `DISABLE_TOOLS`. When `TOOLS_TO_RUN` is not set, start with all tools, remove `DISABLE_TOOLS`, then add `ENABLE_TOOLS`.
- **Clone:** Requires both `ENABLE_LOCAL=true` and `ENABLE_CLONE=true`.
- **LSP:** Requires `ENABLE_LOCAL=true`. When `OCTOCODE_LSP_CONFIG` is unset, Octocode checks `<workspace>/.octocode/lsp-servers.json` then `${OCTOCODE_HOME:-~/.octocode}/lsp-servers.json`.
- **WORKSPACE_ROOT and LSP:** LSP tools read `WORKSPACE_ROOT` from the environment only (not `.octocoderc`). Set it as an env variable in your MCP client if you use LSP tools.
- **Auth tokens:** Never store in `.octocoderc`. GitHub fallback chain: env vars > `~/.octocode/credentials.json` > `gh auth token`.

---

## How to Set Each Option

Every option (except auth-only and advanced-only) can be set in **two places**.

### In MCP Client Settings (`mcp.json` / `claude_desktop_config.json`)

All values are strings in the `"env"` block:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_xxxxxxxxxxxx",
        "GITHUB_API_URL": "https://api.github.com",
        "ENABLE_LOCAL": "true",
        "ENABLE_CLONE": "true",
        "WORKSPACE_ROOT": "/Users/me/projects",
        "ALLOWED_PATHS": "/Users/me/projects,/Users/me/libs",
        "TOOLS_TO_RUN": "githubSearchCode,githubGetFileContent",
        "ENABLE_TOOLS": "localSearchCode",
        "DISABLE_TOOLS": "packageSearch",
        "DISABLE_PROMPTS": "false",
        "REQUEST_TIMEOUT": "30000",
        "MAX_RETRIES": "3",
        "LOG": "true",
        "OCTOCODE_LSP_CONFIG": "/Users/me/.octocode/lsp-servers.json",
        "OCTOCODE_OUTPUT_FORMAT": "yaml",
        "OCTOCODE_HOME": "/Users/me/.octocode",
        "OCTOCODE_MAX_CACHE_SIZE": "2147483648",
        "OCTOCODE_MAX_CLONES": "50"
      }
    }
  }
}
```

### In `.octocoderc` Config File (`~/.octocode/.octocoderc`)

Values use native JSON types (booleans, numbers, arrays — not strings). See the [complete schema above](#2-the-octocoderc-config-file-persistent-defaults) for all fields and defaults.

### Key Differences Between the Two Formats

| | MCP env (`"env"` block) | `.octocoderc` file |
|---|---|---|
| **All values are** | Strings (`"true"`, `"30000"`, `"a,b,c"`) | Native JSON types (`true`, `30000`, `["a","b","c"]`) |
| **Lists** | Comma-separated string: `"a,b,c"` | JSON array: `["a", "b", "c"]` |
| **Booleans** | `"true"` / `"false"` | `true` / `false` |
| **Numbers** | `"30000"` | `30000` |
| **Auth tokens** | Supported | Not supported (never store tokens here) |
| **Scope** | Per-project / per-session | Machine-wide (all sessions) |
| **Priority** | Highest (always wins) | Fallback |

---

## Full Examples

### Minimal Setup (GitHub + remote tools only)

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_xxxxxxxxxxxx"
      }
    }
  }
}
```

### Full-Featured Setup (local + clone + LSP)

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_xxxxxxxxxxxx",
        "ENABLE_LOCAL": "true",
        "ENABLE_CLONE": "true"
      }
    }
  }
}
```

### GitHub Enterprise

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_xxxxxxxxxxxx",
        "GITHUB_API_URL": "https://github.mycompany.com/api/v3"
      }
    }
  }
}
```

### GitLab (self-hosted)

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"],
      "env": {
        "GITLAB_TOKEN": "glpat-xxxxxxxxxxxx",
        "GITLAB_HOST": "https://gitlab.mycompany.com"
      }
    }
  }
}
```

### Bitbucket Cloud

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"],
      "env": {
        "BITBUCKET_TOKEN": "your-app-password",
        "BITBUCKET_USERNAME": "your-username"
      }
    }
  }
}
```

### Production Hardening

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_xxxxxxxxxxxx",
        "ENABLE_LOCAL": "true",
        "REQUEST_TIMEOUT": "60000",
        "MAX_RETRIES": "5",
        "LOG": "false"
      }
    }
  }
}
```

### Restricted Tool Set (only GitHub search tools)

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_xxxxxxxxxxxx",
        "TOOLS_TO_RUN": "githubSearchCode,githubGetFileContent,githubViewRepoStructure,githubSearchRepositories"
      }
    }
  }
}
```

### Combining MCP env + `.octocoderc`

Set persistent defaults in `.octocoderc`:

```jsonc
// ~/.octocode/.octocoderc
{
  "network": { "timeout": 60000, "maxRetries": 5 },
  "local": { "enabled": true, "allowedPaths": ["/Users/me/projects"] }
}
```

Then override per-project in your MCP client:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_xxxxxxxxxxxx",
        "WORKSPACE_ROOT": "/Users/me/projects/my-app",
        "ENABLE_CLONE": "true"
      }
    }
  }
}
```

The env values override `.octocoderc` where they overlap; `.octocoderc` fills in the rest.

---

## Troubleshooting

| Problem | Solution |
|---------|----------|
| Token not found | See your provider's setup guide: [GitHub](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/GITHUB_SETUP_GUIDE.md), [GitLab](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/GITLAB_SETUP_GUIDE.md), [Bitbucket](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/BITBUCKET_SETUP_GUIDE.md) |
| Local tools not showing | Check `ENABLE_LOCAL` is not set to `false` in MCP `"env"` or `.octocoderc` |
| Clone/directory tools disabled | Set both `ENABLE_LOCAL=true` and `ENABLE_CLONE=true` |
| Wrong provider active | Check which tokens are set — GitLab > Bitbucket > GitHub |
| Timeout errors | Increase `REQUEST_TIMEOUT` (max `300000`) |
| Tool not available | Check if `TOOLS_TO_RUN` or `DISABLE_TOOLS` is filtering it out |
| Config file ignored | Env variables always override `.octocoderc` — check your MCP `"env"` block |
| Config changes not applied | Restart the MCP server (config is read at startup) |
| Usage stats missing | Run at least one Octocode tool, then check `${OCTOCODE_HOME:-~/.octocode}/stats.json`. |

### Verify Your Setup

```bash
echo "GITHUB_TOKEN: ${GITHUB_TOKEN:+set}"
echo "GITLAB_TOKEN: ${GITLAB_TOKEN:+set}"
echo "BITBUCKET_TOKEN: ${BITBUCKET_TOKEN:+set}"
echo "ENABLE_LOCAL: ${ENABLE_LOCAL:-not set}"
echo "LOG: ${LOG:-not set}"

ls -la ~/.octocode/.octocoderc
cat ~/.octocode/.octocoderc | python3 -c "import sys,json; json.load(sys.stdin)"
if [ -f "${OCTOCODE_HOME:-$HOME/.octocode}/stats.json" ]; then
  cat "${OCTOCODE_HOME:-$HOME/.octocode}/stats.json" | python3 -c "import sys,json; json.load(sys.stdin)"
fi
```

---

## See Also

- [Authentication Setup](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md) — Provider authentication overview
- [GitHub Setup Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/GITHUB_SETUP_GUIDE.md) — GitHub auth, Enterprise, clone tools
- [GitLab Setup Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/GITLAB_SETUP_GUIDE.md) — GitLab auth, self-hosted, tier limits
- [Bitbucket Setup Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/BITBUCKET_SETUP_GUIDE.md) — Bitbucket auth, app passwords, OAuth
- [GitHub, GitLab & Bitbucket Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/GITHUB_GITLAB_TOOLS_REFERENCE.md) — Remote code research tools
- [Local & LSP Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LOCAL_TOOLS_REFERENCE.md) — Local tools (`ENABLE_LOCAL`)
- [Troubleshooting](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/TROUBLESHOOTING.md) — Node.js, npm, and connection issues
