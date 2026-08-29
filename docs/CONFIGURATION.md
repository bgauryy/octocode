# Octocode configuration and authentication

## Table of contents

- [Quick setup](#quick-setup)
- [Authentication](#authentication)
  - [Method 1 — Octocode OAuth login](#method-1--octocode-oauth-login-recommended)
  - [Method 2 — Token env var](#method-2--token-env-var)
  - [Method 3 — gh CLI passthrough](#method-3--gh-cli-passthrough)
  - [Token priority order](#token-priority-order)
  - [Auth commands](#auth-commands)
- [Config files](#config-files)
  - [Where everything lives](#where-everything-lives)
  - [`.env` — third-party API keys](#env--third-party-api-keys)
  - [`.octocoderc` — Octocode settings](#octocoderc--octocode-settings)
  - [How settings override each other](#how-settings-override-each-other)
- [MCP client `env` block](#mcp-client-env-block)
- [All settings reference](#all-settings-reference)
  - [Third-party keys](#third-party-keys)
  - [GitHub token](#github-token)
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
- [See also](#see-also)

---

## Quick setup

```bash
# Step 1 — authenticate (opens browser, stores encrypted token)
npx octocode auth login

# Step 2 — (optional) add web search for better results
echo 'TAVILY_API_KEY=tvly-...' >> ~/.octocode/.env

# Step 3 — verify
npx octocode status --json
```

Already have a GitHub token and don't want a browser login? See [Method 2 — Token env var](#method-2--token-env-var).

---

## Authentication

Octocode needs a GitHub token to search code, read files, and call the GitHub API. Three ways to provide one follow; pick the one that fits your workflow.

---

### Method 1 — Octocode OAuth login (recommended)

**Best for:** individual developers, local use, any time a browser is available.

```bash
npx octocode auth login
```

- Opens GitHub's OAuth Device Flow in your browser.
- Octocode stores the token **AES-256-GCM encrypted** at `~/.octocode/credentials.json` (key at `~/.octocode/.key`, both `chmod 600`).
- GitHub App tokens auto-refresh. Standard `ghp_*` personal access tokens don't expire.
- Octocode reads it automatically on every request — nothing else to configure.

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
      "args": ["-y", "octocode-mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_..."
      }
    }
  }
}
```

> ⚠️ **Tokens cannot go in `~/.octocode/.env`** — all four token vars are protected keys, and the loader skips them without warning. Use your shell, your shell profile (`~/.zshrc`, `~/.bashrc`), or the MCP `env` block.

Changes take effect on the **next request** — no restart needed.

---

### Method 3 — gh CLI passthrough

**Best for:** developers who already use the [GitHub CLI (`gh`)](https://cli.github.com/) and don't want to manage a second token.

```bash
gh auth login     # one-time setup with the gh CLI
```

That's it. When Octocode finds no other token, it calls `gh auth token` as a fallback. Nothing to configure in Octocode.

---

### Token priority order

Octocode checks these sources in order and stops at the first non-empty value. The following table lists them by priority:

| # | Type | Source | How to set |
|---|------|--------|-----------|
| 1 | Env var | `OCTOCODE_TOKEN` | `export OCTOCODE_TOKEN=ghp_...` |
| 2 | Env var | `GH_TOKEN` | `export GH_TOKEN=ghp_...` |
| 3 | Env var | `GITHUB_TOKEN` | `export GITHUB_TOKEN=ghp_...` · auto-set in GitHub Actions |
| 4 | Env var | `GITHUB_PERSONAL_ACCESS_TOKEN` | `export GITHUB_PERSONAL_ACCESS_TOKEN=ghp_...` |
| 5 | Octocode OAuth | encrypted storage | `npx octocode auth login` |
| 6 | gh CLI | `gh auth token` | `gh auth login` |

**Env vars always beat stored credentials.** When a token env var is set, Octocode ignores the stored token.

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

## Config files

### Where everything lives

All Octocode config, credentials, cache, and session data live under one directory — the **Octocode home**. On every platform, that directory is `.octocode` inside the OS home directory: `~/.octocode` on macOS and Linux, `%USERPROFILE%\.octocode` on Windows. Octocode does not read `XDG_CONFIG_HOME` or `APPDATA` for this path.

To override it for all products at once, set `OCTOCODE_HOME`:

```bash
export OCTOCODE_HOME=/custom/path
```

The following table lists the persistent files and cache directories inside the home directory:

| Path | What it does |
|------|-------------|
| `.env` | Your third-party API keys (Tavily, Serper, …). Loaded by agents and skills. |
| `.octocoderc` | Octocode behavior settings (tools, network, paths, output). Read by the MCP server and CLI. |
| `credentials.json` | Encrypted GitHub token from `octocode auth login`. Don't edit manually. |
| `stats.json` | Usage counters (tool calls, cache hits, …). Written only when `OCTOCODE_ENABLE_STATS=1`. |
| `session.json` | Session identity. |
| `tmp/clone/` | Git clones, keyed by owner, repository, and branch/sparse identity. |
| `tmp/tree/` | API-materialized files and directories, keyed by owner, repository, and immutable commit SHA. |
| `tmp/response/` | Shared file-backed L2 cache for eligible GitHub and package responses. |

### Cache storage and lifecycle

The CLI and MCP share the same cache roots under `OCTOCODE_HOME`; neither product uses editor-local storage for remote materialization.

| Concern | Behavior |
|---------|----------|
| Automatic cleanup | A persisted due marker allows one full maintenance pass per 24-hour window across processes. A cross-process lock prevents duplicate clone, tree, and response scans. |
| CLI | A tool that enters the direct execution runtime performs the due-check once per process. The CLI does not start a background timer, so the check cannot keep a command alive. Help, schema, and context-only paths remain side-effect free. |
| MCP | Server initialization performs the same due-check. After the stdio transport connects, MCP schedules the next persisted deadline with an `unref()` timer and cancels it during shutdown. |
| Expiry | Clone and tree entries use the clone TTL (24 hours by default). Response entries carry their own fresh/stale deadlines and are removed after the stale deadline. Stale clone temp artifacts and locks are also recovered. |
| Failure | Maintenance is best-effort. An unavailable or read-only cache home does not prevent CLI tool execution or MCP startup. |
| Ownership | Automatic maintenance traverses only Octocode-owned `clone`, `tree`, clone-artifact, and `response` roots. It preserves unrelated directories under `tmp/`. |
| Manual inspection | `octocode cache status` reports total `tmp`, clone, tree, and response sizes. |
| Manual clear | `octocode cache clear --clone` and `--tree` are selective. `octocode cache clear --all` removes the entire `<octocode-home>/tmp/` directory, including response entries and lifecycle metadata; there is currently no response-only clear flag. |

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

# Tavily — AI-curated, deeper research
# Get key → https://app.tavily.com/
TAVILY_API_KEY=tvly-...

# Serper — broad Google SERP results
# Get key → https://serper.dev/
SERPER_API_KEY=...

# Exa — neural search with category filters (papers, GitHub, news)
# Get key → https://dashboard.exa.ai/
EXA_API_KEY=...

# ── Any other keys your skills need ─────────────────────────────────────────
EXAMPLE_SKILL_KEY=...
```

The following rules apply:
- A key already set in your shell wins over this file.
- A project `.env` at `<project>/.octocode/.env` overrides the global file for matching keys (only loaded for trusted projects).
- **Agent sessions and skill scripts** load this file automatically. The MCP server and CLI do **not** load it — pass those keys through your shell or the MCP `env` block instead.
- Octocode blocks the GitHub token vars (`OCTOCODE_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`, `GITHUB_PERSONAL_ACCESS_TOKEN`) here — put them in your shell.

Skills query every web-search engine whose key is set and validated, then fuse the results — Serper returns raw Google results, Tavily returns curated summaries, and Exa returns neural and category-filtered results, so they are not interchangeable. When no key is set, skills fall back to DuckDuckGo, which needs no key.

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

**After editing:** restart your MCP server or start a new agent session so the changes take effect.

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
    // false → turn off all local filesystem tools (localSearchCode, localFindFiles, …)
    "enabled": true,

    // true → turn on ghCloneRepo (clone a GitHub repo to disk for deep local analysis)
    // CLI default: true  |  MCP default: false (must opt in)
    "enableClone": false,

    // Lock the workspace root to a specific path (default: process.cwd())
    // Must be an absolute path. Example: "/home/user/projects"
    "workspaceRoot": null,

    // Extra directories to allow, ADDED on top of the always-allowed home dir.
    // Empty = home directory only (paths outside home are denied).
    // Example: ["/home/user/projects", "/tmp/sandbox"]
    "allowedPaths": []
  },

  // ── Tool availability ─────────────────────────────────────────────────────
  "tools": {
    // Strict allowlist — only these tools are registered. Overrides enabled/disabled.
    // null = use the default tool set
    // Example: ["ghSearchCode", "localSearchCode", "npmSearch"]
    "enabled": null,

    // Add specific tools on top of the default set.
    // Example: ["ghCloneRepo"]
    "enableAdditional": null,

    // Remove specific tools from the default set.
    // Example: ["ghCloneRepo"]
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

Every setting in `.octocoderc` also has an **env var** — env vars always win. For the env var name of each option, see [All settings reference](#all-settings-reference).

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

Three rules follow from this order:
- **Env vars always beat file config.** Set an env var and `.octocoderc` is ignored for that setting.
- **`.env` is only for agent/skill sessions.** The MCP server and CLI don't load it — use your shell or the MCP `env` block.
- **GitHub tokens never come from `.env`** — they're blocked there regardless of priority.

---

## MCP client `env` block

Configure the MCP server without touching a shell profile: pass env vars directly in your client config file.

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"],
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

To write this automatically, run `npx octocode install --ide cursor`. The `--ide` flag also accepts `vscode`, `claude`, and `windsurf`, among others.

---

## All settings reference

### Third-party keys

Set these in `~/.octocode/.env` or in your shell. Skills read them; the MCP server and CLI do not. The following table lists each key and its default:

| Key | Default | Notes |
|-----|---------|-------|
| `TAVILY_API_KEY` | unset | Web search — curated, deeper research. [Get a Tavily key](https://app.tavily.com/) |
| `SERPER_API_KEY` | unset | Web search — broad Google SERP results. [Get a Serper key](https://serper.dev/) |
| `EXA_API_KEY` | unset | Web search — neural search with category filters. [Get an Exa key](https://dashboard.exa.ai/) |

---

### Octocode settings — env var or `~/.octocode/.octocoderc`

#### GitHub token

Set the GitHub token in an environment variable only. Octocode never reads it from `.env` or `.octocoderc`.

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
| `ENABLE_LOCAL` | `local.enabled` | CLI `true`; MCP resolver default `false`, but the built MCP server registered local tools without it | Set it explicitly on MCP. `false` turns local tools off on every surface |
| `ENABLE_CLONE` | `local.enableClone` | CLI `true`, MCP `false` | Turns on `ghCloneRepo` |
| `ENABLE_RELEASES` | — (env-only) | `false` | Adds `ghListReleases` to the catalog. Registers it on the CLI; on MCP, also list it in `ENABLE_TOOLS` |
| `ENABLE_DISCUSSIONS` | — (env-only) | `false` | Adds `ghSearchDiscussions` to the catalog. Registers it on the CLI; on MCP, also list it in `ENABLE_TOOLS` |
| `WORKSPACE_ROOT` | `local.workspaceRoot` | `process.cwd()` | Must be absolute. Base for resolving relative paths — not itself an allowed root; add it to `allowedPaths` to access a location outside home. |
| `ALLOWED_PATHS` | `local.allowedPaths` | `[]` (home only) | Extra roots added on top of the always-allowed home directory. Env: comma-separated; rc: JSON array. |

#### Tools

| Env var | `.octocoderc` key | Default | Notes |
|---------|------------------|---------|-------|
| `TOOLS_TO_RUN` | `tools.enabled` | `null` | Strict allowlist — overrides add/remove |
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
| `OCTOCODE_HOME` | `<os-home>/.octocode` | Overrides the config directory for all products |

---

### Advanced runtime — env var only

`octocode-tools-core` reads these lower-level knobs directly. They have **no** `.octocoderc` equivalent — set them in your shell or MCP `env` block.

#### Stats persistence

| Env var | Default | Notes |
|---------|---------|-------|
| `OCTOCODE_ENABLE_STATS` | `false` (off) | Set to `1` or `true` to write `stats.json` on every flush. Octocode tracks stats in memory either way; this setting controls only whether it writes them to disk. Keeping it off eliminates one file write per 60-second flush cycle, which reduces SSD wear on long-running agent sessions. |

#### Clone and tree cache

| Env var | Default | Notes |
|---------|---------|-------|
| `OCTOCODE_CACHE_TTL_MS` | `86400000` (24 h) | How long a cloned repository stays fresh before re-fetch |
| `OCTOCODE_MAX_CACHE_SIZE` | `2147483648` (2 GB) | Total byte cap for the clone cache on disk |
| `OCTOCODE_MAX_CLONES` | `50` | Maximum number of repositories the clone cache keeps |

#### Response cache

Eligible GitHub and package operations use a shared memory L1 plus the file-backed `tmp/response/` L2. Each entry defines its own fresh and stale deadlines; the settings below control storage rather than response TTL.

| Env var | Default | Notes |
|---------|---------|-------|
| `OCTOCODE_DISK_CACHE` | `true` | Set to `false` or `0` to disable response-cache disk reads and writes. |
| `OCTOCODE_DISK_CACHE_MAX_ENTRY_SIZE` | `5242880` (5 MiB) | Maximum serialized size of one response entry. |
| `OCTOCODE_DISK_CACHE_MAX_ENTRIES` | `5000` | Maximum live response entries retained by maintenance. |
| `OCTOCODE_DISK_CACHE_MAX_SIZE` | `268435456` (256 MiB) | Maximum total bytes retained under `tmp/response/`. |

#### Timeouts

| Env var | Default | Notes |
|---------|---------|-------|
| `OCTOCODE_TOOL_TIMEOUT_MS` | `60000` (60 s) | Hard wall-clock timeout for a single tool call |
| `OCTOCODE_BULK_QUERY_TIMEOUT_MS` | `60000` (60 s) | Timeout for a bulk / multi-query tool operation |

---

### Protected keys — never sourced from `.env`

Octocode **always ignores** these keys when loading `~/.octocode/.env` or a project `.env`, whatever their values. Set them in your shell, CI environment, or the MCP `env` block instead. The following table lists each key and the reason it is protected:

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
| `NODE_OPTIONS` | Node.js runtime flags — a security risk if `.env` could override them |
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
| `ghListReleases` or `ghSearchDiscussions` unavailable in MCP | The feature flag alone is not enough. Also add `"ENABLE_TOOLS": "ghListReleases,ghSearchDiscussions"` |
| Local tools turned off | On MCP, set `ENABLE_LOCAL=true`. Otherwise, check that neither `ENABLE_LOCAL` nor `local.enabled` is `false` |
| A tool is missing | Check `TOOLS_TO_RUN` (strict allowlist), `ENABLE_TOOLS`, `DISABLE_TOOLS` |
| Slow / timeouts | Raise `REQUEST_TIMEOUT` (max `300000` ms) |
| Web search low quality | Add `TAVILY_API_KEY` to `~/.octocode/.env` |
| `stats.json` never written | Set `OCTOCODE_ENABLE_STATS=1` in your shell or MCP `env` block (off by default) |
| `.env` key ignored | Octocode blocks token vars in `.env` — use your shell or the MCP `env` block |
| `.env` key not loading | Confirm the agent session restarted and the project is trusted |
| Enterprise hitting github.com | Set `GITHUB_API_URL` in both shell and `.octocoderc` |
| Settings not taking effect | Restart the MCP server or start a new agent session after editing `.octocoderc` |

---

## See also

- [Octocode tools reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md) — all tools and parameters
- [Octocode MCP server](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md) — startup lifecycle and client config
- [Octocode CLI guide](https://github.com/bgauryy/octocode/blob/main/packages/octocode/docs/OCTOCODE_CLI.md) — all CLI commands
- [LSP server lifecycle](https://github.com/bgauryy/octocode/blob/main/packages/octocode-engine/docs/LSP_SERVER_LIFECYCLE.md) — custom language server config
- [Security](https://github.com/bgauryy/octocode/blob/main/docs/SECURITY.md) — secret redaction and path validation
