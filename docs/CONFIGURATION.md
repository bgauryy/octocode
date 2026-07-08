# Octocode Configuration & Authentication

## Table of Contents

- [Quick Setup](#quick-setup)
- [Authentication](#authentication)
  - [Method 1 — Octocode OAuth login](#method-1--octocode-oauth-login-recommended)
  - [Method 2 — Token env var](#method-2--token-env-var)
  - [Method 3 — gh CLI passthrough](#method-3--gh-cli-passthrough)
  - [Token priority order](#token-priority-order)
  - [Auth commands](#auth-commands)
- [Config Files](#config-files)
  - [Where everything lives](#where-everything-lives)
  - [`.env` — third-party API keys](#env--third-party-api-keys)
  - [`.octocoderc` — Octocode settings](#octocoderc--octocode-settings)
  - [How settings override each other](#how-settings-override-each-other)
- [MCP Client `env` Block](#mcp-client-env-block)
- [All Settings Reference](#all-settings-reference)
  - [Third-party keys](#third-party-keys----octocodeenv-or-shell)
  - [GitHub token](#github-token-env-var-only----not-in-env-or-octocoderc)
  - [GitHub API](#github-api)
  - [Local tools](#local-tools)
  - [Tools](#tools)
  - [Network](#network)
  - [Output](#output)
  - [LSP](#lsp)
  - [Home directory](#home-directory)
  - [Advanced runtime](#advanced-runtime--env-var-only)
  - [Protected keys](#protected-keys--never-sourced-from-env)
- [GitHub Enterprise](#github-enterprise)
- [Troubleshooting](#troubleshooting)
- [See Also](#see-also)

---

## Quick Setup

```bash
# Step 1 — authenticate (opens browser, stores encrypted token)
npx octocode auth login

# Step 2 — (optional) add web search for better results
echo 'TAVILY_API_KEY=tvly-...' >> ~/.octocode/.env

# Step 3 — verify
npx octocode status --json
```

Already have a GitHub token and don't want a browser login? Jump to [Method 2](#method-2--token-env-var).

---

## Authentication

Octocode needs a GitHub token to search code, read files, and call the GitHub API. There are **three ways** to provide one — pick the one that fits your workflow.

---

### Method 1 — Octocode OAuth login (recommended)

**Best for:** individual developers, local use, any time a browser is available.

```bash
npx octocode auth login
```

- Opens GitHub's OAuth Device Flow in your browser.
- Token is stored **AES-256-GCM encrypted** at `~/.octocode/credentials.json` (key at `~/.octocode/.key`, both `chmod 600`).
- GitHub App tokens auto-refresh. Standard `ghp_*` personal access tokens don't expire.
- Octocode reads this automatically on every request — nothing else to configure.

```bash
npx octocode auth login --force      # replace an existing stored token
npx octocode auth logout             # delete the stored token
```

---

### Method 2 — Token env var

**Best for:** CI/CD, MCP clients, scripts, or anywhere you already manage tokens as env vars.

Set any one of these in your shell, CI environment, or MCP client `env` block:

```bash
# In your shell or ~/.zshrc / ~/.bashrc
export GITHUB_TOKEN=ghp_...

# Or use the Octocode-specific var (highest priority)
export OCTOCODE_TOKEN=ghp_...
```

**In an MCP client config file** (no shell export needed):

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "@octocodeai/mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_..."
      }
    }
  }
}
```

> ⚠️ **Tokens cannot go in `~/.octocode/.env`** — all four token vars are protected keys and will be silently skipped. Use your shell, your shell profile (`~/.zshrc`, `~/.bashrc`), or the MCP `env` block.

Changes take effect on the **next request** — no restart needed.

---

### Method 3 — gh CLI passthrough

**Best for:** developers who already use the [GitHub CLI (`gh`)](https://cli.github.com/) and don't want to manage a second token.

```bash
gh auth login     # one-time setup with the gh CLI
```

That's it. Octocode automatically calls `gh auth token` as a fallback when no other token is found. Nothing to configure in Octocode.

---

### Token priority order

Octocode checks these sources in order and stops at the first non-empty value:

| # | Type | Source | How to set |
|---|------|--------|-----------|
| 1 | Env var | `OCTOCODE_TOKEN` | `export OCTOCODE_TOKEN=ghp_...` |
| 2 | Env var | `GH_TOKEN` | `export GH_TOKEN=ghp_...` |
| 3 | Env var | `GITHUB_TOKEN` | `export GITHUB_TOKEN=ghp_...` · auto-set in GitHub Actions |
| 4 | Env var | `GITHUB_PERSONAL_ACCESS_TOKEN` | `export GITHUB_PERSONAL_ACCESS_TOKEN=ghp_...` |
| 5 | Octocode OAuth | encrypted storage | `npx octocode auth login` |
| 6 | gh CLI | `gh auth token` | `gh auth login` |

**Env vars always beat stored credentials.** If a token env var is set, the stored token is ignored.

---

### Auth commands

```bash
npx octocode auth login              # OAuth — opens browser, saves encrypted token
npx octocode auth login --force      # replace the existing stored token
npx octocode auth login --hostname github.mycompany.com  # GitHub Enterprise OAuth
npx octocode auth logout             # delete the stored token
npx octocode auth status             # show token source + GitHub username
npx octocode auth status --json      # machine-readable
npx octocode status --json           # full status: token + tools + config
```

---

## Config Files

### Where everything lives

All Octocode config, credentials, cache, and session data live under one directory — the **Octocode home**:

| Platform | Default location |
|----------|-----------------|
| macOS | `~/.octocode/` |
| Linux | `${XDG_CONFIG_HOME:-~/.config}/.octocode/` |
| Windows | `%APPDATA%\.octocode\` |

Override it for all products at once:

```bash
export OCTOCODE_HOME=/custom/path
```

**Files inside the home directory:**

| File | What it does |
|------|-------------|
| `.env` | Your third-party API keys (Tavily, Serper, …). Loaded by agents and skills. |
| `.octocoderc` | Octocode behavior settings (tools, network, paths, output). Read by the MCP server and CLI. |
| `credentials.json` | Encrypted GitHub token from `octocode auth login`. Don't edit manually. |
| `stats.json` | Usage counters (tool calls, cache hits, …). |
| `session.json` | Session identity. |

---

### `.env` — third-party API keys

**What it is:** A plain key=value file for third-party API keys used by Octocode's web search and any agent skills you install. It is **not** for Octocode's own settings.

**Where:** `~/.octocode/.env` (global) · `<project>/.octocode/.env` (project-level, overrides global)

**How to create or edit it:**

```bash
# Create the directory if it doesn't exist
mkdir -p ~/.octocode

# Add a key (append, or open in any text editor)
echo 'TAVILY_API_KEY=tvly-...' >> ~/.octocode/.env
echo 'SERPER_API_KEY=...'      >> ~/.octocode/.env

# Or open in your editor
nano ~/.octocode/.env
code ~/.octocode/.env
```

**File format — plain KEY=VALUE:**

```bash
# ~/.octocode/.env

# ── Web search ────────────────────────────────────────────────────────────────

# Tavily — AI-curated results (recommended). TAVILY_API_TOKEN is an alias.
# Get key → https://app.tavily.com/
TAVILY_API_KEY=tvly-...

# Serper — Google SERP results (fallback after Tavily)
# Get key → https://serper.dev/
SERPER_API_KEY=...

# ── Web fetch ────────────────────────────────────────────────────────────────

# Override the User-Agent sent by the web browse/fetch tool.
# API providers (Tavily, Serper) ignore this — they use their own auth.
# Default: Chrome-like UA
OCTOCODE_WEB_USER_AGENT=MyBot/1.0

# ── Any other keys your skills need ─────────────────────────────────────────
MY_CUSTOM_KEY=...
```

**Rules:**
- Keys already set in your shell always win over this file.
- A project `.env` at `<project>/.octocode/.env` overrides the global file for matching keys (only loaded for trusted projects).
- This file is loaded automatically by **agent sessions and skill scripts**. The MCP server and CLI do **not** load it — pass those keys via shell or the MCP `env` block instead.
- GitHub token vars (`OCTOCODE_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`, `GITHUB_PERSONAL_ACCESS_TOKEN`) are blocked here — put them in your shell.

Web search fallback order: **Tavily → Serper → DuckDuckGo** (DuckDuckGo works with no key).

---

### `.octocoderc` — Octocode settings

**What it is:** A JSON config file for Octocode's own behavior — tool availability, network settings, local path restrictions, output format, LSP config. It is **not** for third-party API keys.

**Where:** `~/.octocode/.octocoderc`

**How to create or edit it:**

```bash
# Create the directory if it doesn't exist
mkdir -p ~/.octocode

# Open in your editor — it's JSON with comments (JSONC)
nano ~/.octocode/.octocoderc
code ~/.octocode/.octocoderc
```

**After editing:** restart your MCP server or start a new agent session for changes to take effect.

**Full reference file with every option:**

```jsonc
// ~/.octocode/.octocoderc
// JSON with comments and trailing commas are both supported.
{
  // ── GitHub ────────────────────────────────────────────────────────────────
  "github": {
    // Default: "https://api.github.com"
    // GitHub Enterprise: "https://ghe.mycompany.com/api/v3"
    "apiUrl": "https://api.github.com"
  },

  // ── Local filesystem tools ────────────────────────────────────────────────
  "local": {
    // false → disable all local filesystem tools (localSearchCode, localFindFiles, …)
    "enabled": true,

    // true → enable ghCloneRepo (clone a GitHub repo to disk for deep local analysis)
    // CLI default: true  |  MCP default: false (must opt in)
    "enableClone": false,

    // Lock the workspace root to a specific path (default: process.cwd())
    // Must be an absolute path. Example: "/home/user/projects"
    "workspaceRoot": null,

    // Restrict local tools to these paths only. Empty = unrestricted.
    // Example: ["/home/user/projects", "/tmp/sandbox"]
    "allowedPaths": []
  },

  // ── Tool availability ─────────────────────────────────────────────────────
  "tools": {
    // Strict whitelist — only these tools are registered. Overrides enabled/disabled.
    // null = use the default tool set
    // Example: ["ghSearchCode", "localSearchCode", "npmSearch"]
    "enabled": null,

    // Add specific tools on top of the default set.
    // Example: ["ghCloneRepo"]
    "enableAdditional": null,

    // Remove specific tools from the default set.
    // Example: ["ghCloneRepo", "localBinaryInspect"]
    "disabled": null
  },

  // ── Network ───────────────────────────────────────────────────────────────
  "network": {
    // Request timeout in milliseconds. Range: 5000–300000. Default: 30000
    "timeout": 30000,

    // Max retries on failure. Range: 0–10. Default: 3
    "maxRetries": 3
  },

  // ── Output ────────────────────────────────────────────────────────────────
  "output": {
    // Response format: "yaml" (default) or "json"
    "format": "yaml",

    "pagination": {
      // Auto-pagination character budget. Range: 1000–50000. Default: 20000
      "defaultCharLength": 20000
    }
  },

  // ── LSP ───────────────────────────────────────────────────────────────────
  "lsp": {
    // Path to a custom lsp-servers.json. null = use built-in defaults.
    "configPath": null
  }
}
```

Every setting in `.octocoderc` can also be set via an **env var** — env vars always win. See [All Settings Reference](#all-settings-reference) for the env var name for each option.

---

### How settings override each other

```
Shell env vars / MCP client env block       ← always win, highest priority
  ↓
<project>/.octocode/.env                    ← project API keys (agent/skills only)
~/.octocode/.env                            ← global API keys  (agent/skills only)
  ↓
~/.octocode/.octocoderc                     ← Octocode settings (MCP server + CLI)
  ↓
Built-in defaults
```

Key takeaways:
- **Env vars always beat file config.** Set an env var and `.octocoderc` is ignored for that setting.
- **`.env` is only for agent/skill sessions.** The MCP server and CLI don't load it — use your shell or the MCP `env` block.
- **GitHub tokens never come from `.env`** — they're blocked there regardless of priority.

---

## MCP Client `env` Block

The cleanest way to configure the MCP server — no shell profile changes needed. Pass env vars directly in your client config file:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "@octocodeai/mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_...",
        "ENABLE_CLONE": "true",
        "REQUEST_TIMEOUT": "60000",
        "GITHUB_API_URL": "https://ghe.mycompany.com/api/v3"
      }
    }
  }
}
```

Run `npx octocode install --ide cursor` (or `vscode`, `claude`, `windsurf`, etc.) to write this automatically.

---

## All Settings Reference

### Third-party keys — `~/.octocode/.env` or shell

| Key | Default | Notes |
|-----|---------|-------|
| `TAVILY_API_KEY` | unset | Web search — AI-curated. [Get key →](https://app.tavily.com/) |
| `TAVILY_API_TOKEN` | unset | Alias for `TAVILY_API_KEY` — either works |
| `SERPER_API_KEY` | unset | Web search — Google SERP. [Get key →](https://serper.dev/) |
| `OCTOCODE_WEB_USER_AGENT` | Chrome-like UA | User-Agent for the web fetch tool. API providers ignore it (they use their own auth headers). |

---

### Octocode settings — env var or `~/.octocode/.octocoderc`

#### GitHub token (env var only — not in `.env` or `.octocoderc`)

| Env var | Priority | Notes |
|---------|----------|-------|
| `OCTOCODE_TOKEN` | 1 — highest | Octocode-specific override |
| `GH_TOKEN` | 2 | |
| `GITHUB_TOKEN` | 3 | Auto-set in GitHub Actions |
| `GITHUB_PERSONAL_ACCESS_TOKEN` | 4 | |

#### GitHub API

| Env var | `.octocoderc` key | Default |
|---------|------------------|---------|
| `GITHUB_API_URL` | `github.apiUrl` | `https://api.github.com` |

#### Local tools

| Env var | `.octocoderc` key | Default | Notes |
|---------|------------------|---------|-------|
| `ENABLE_LOCAL` | `local.enabled` | `true` | `false` → disable all local tools |
| `ENABLE_CLONE` | `local.enableClone` | CLI: `true` · MCP: `false` | Enable `ghCloneRepo` |
| `WORKSPACE_ROOT` | `local.workspaceRoot` | `process.cwd()` | Must be absolute |
| `ALLOWED_PATHS` | `local.allowedPaths` | `[]` unrestricted | Env: comma-separated; rc: JSON array |

#### Tools

| Env var | `.octocoderc` key | Default | Notes |
|---------|------------------|---------|-------|
| `TOOLS_TO_RUN` | `tools.enabled` | `null` | Strict whitelist — overrides add/remove |
| `ENABLE_TOOLS` | `tools.enableAdditional` | `null` | Add tools to the default set |
| `DISABLE_TOOLS` | `tools.disabled` | `null` | Remove tools from the default set |

#### Network

| Env var | `.octocoderc` key | Default | Range |
|---------|------------------|---------|-------|
| `REQUEST_TIMEOUT` | `network.timeout` | `30000` ms | 5 000 – 300 000 |
| `MAX_RETRIES` | `network.maxRetries` | `3` | 0 – 10 |

#### Output

| Env var | `.octocoderc` key | Default | Notes |
|---------|------------------|---------|-------|
| `OCTOCODE_OUTPUT_FORMAT` | `output.format` | `yaml` | `yaml` or `json` |
| `OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH` | `output.pagination.defaultCharLength` | `20000` | 1 000 – 50 000 |

#### LSP

| Env var | `.octocoderc` key | Default |
|---------|------------------|---------|
| `OCTOCODE_LSP_CONFIG` | `lsp.configPath` | unset |

#### Home directory

| Env var | Default | Notes |
|---------|---------|-------|
| `OCTOCODE_HOME` | Platform default | Overrides the config directory for all products |

---

### Advanced runtime — env var only

These are lower-level knobs read directly by `octocode-tools-core`. They do **not** have a `.octocoderc` equivalent — set them in your shell or MCP `env` block.

#### Clone cache

| Env var | Default | Notes |
|---------|---------|-------|
| `OCTOCODE_CACHE_TTL_MS` | `86400000` (24 h) | How long a cloned repo stays fresh before re-fetch |
| `OCTOCODE_MAX_CACHE_SIZE` | `2147483648` (2 GB) | Total byte cap for the clone cache on disk |
| `OCTOCODE_MAX_CLONES` | `50` | Maximum number of repos that can be kept in the clone cache |

#### Timeouts

| Env var | Default | Notes |
|---------|---------|-------|
| `OCTOCODE_TOOL_TIMEOUT_MS` | `60000` (60 s) | Hard wall-clock timeout for a single tool call |
| `OCTOCODE_BULK_QUERY_TIMEOUT_MS` | `60000` (60 s) | Timeout for a bulk / multi-query tool operation |

---

### Protected keys — never sourced from `.env`

These keys are **always ignored** when loading `~/.octocode/.env` or a project `.env`, regardless of their values. Set them in your shell, CI environment, or the MCP `env` block instead.

| Key | Why protected |
|-----|---------------|
| `OCTOCODE_TOKEN` | GitHub auth — must be explicit |
| `GH_TOKEN` | GitHub auth — must be explicit |
| `GITHUB_TOKEN` | GitHub auth — must be explicit |
| `GITHUB_PERSONAL_ACCESS_TOKEN` | GitHub auth — must be explicit |
| `PATH` | OS binary resolution — `.env` must not hijack it |
| `HOME` | OS home directory — must not be overridden |
| `SHELL` | User login shell |
| `USER` / `LOGNAME` | User identity |
| `PWD` | Working directory |
| `TMPDIR` | System temp directory |
| `NODE_OPTIONS` | Node.js runtime flags — security risk if overrideable |
| `PYTHON` | Python interpreter path |

---

## GitHub Enterprise

```bash
# Shell / CI
export GITHUB_TOKEN="ghp_your_ghe_token"
export GITHUB_API_URL="https://github.mycompany.com/api/v3"

# OAuth login against GHE
npx octocode auth login --hostname github.mycompany.com
```

Or set it permanently in `~/.octocode/.octocoderc`:

```jsonc
{
  "github": { "apiUrl": "https://github.mycompany.com/api/v3" }
}
```

---

## Troubleshooting

Always start here:

```bash
npx octocode status --json
```

| Symptom | Fix |
|---------|-----|
| No token / 401 | Run `npx octocode auth login`, or set `GITHUB_TOKEN` in shell or MCP `env` block |
| Wrong GitHub account | `npx octocode auth logout` then `auth login` — or `auth login --force` |
| Env token overriding saved token | Env always wins — unset the env var |
| `ghCloneRepo` unavailable in MCP | Add `"ENABLE_CLONE": "true"` to the MCP `env` block |
| Local tools disabled | Check `ENABLE_LOCAL` isn't `false` and `local.enabled` isn't `false` |
| A tool is missing | Check `TOOLS_TO_RUN` (strict whitelist), `ENABLE_TOOLS`, `DISABLE_TOOLS` |
| Slow / timeouts | Raise `REQUEST_TIMEOUT` (max `300000` ms) |
| Web search low quality | Add `TAVILY_API_KEY` to `~/.octocode/.env` |
| `.env` key ignored | Token vars are blocked in `.env` — use shell or MCP `env` block |
| `.env` key not loading | Confirm the agent session restarted and the project is trusted |
| Enterprise hitting github.com | Set `GITHUB_API_URL` in both shell and `.octocoderc` |
| Settings not taking effect | Restart the MCP server or start a new agent session after editing `.octocoderc` |

---

## See Also

- [Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/OCTOCODE_TOOLS.md) — all tools and parameters
- [MCP Server](https://github.com/bgauryy/octocode-mcp/blob/main/docs/OCTOCODE_MCP.md) — startup lifecycle and client config
- [CLI Guide](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OCTOCODE_CLI.md) — all CLI commands
- [LSP Setup](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-engine/docs/LSP_SERVER_LIFECYCLE.md) — custom language server config
- [Security](https://github.com/bgauryy/octocode-mcp/blob/main/docs/SECURITY.md) — secret redaction and path validation
