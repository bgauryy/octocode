# Octocode configuration and authentication

Configures credentials, registries, feature gates, storage, timeouts, and environment precedence for the Octocode toolkit. Configuration controls whether a capability is available; it does not redefine a tool's input schema. See [`OCTOCODE_TOOLS.md`](OCTOCODE_TOOLS.md) for tool fields, [`OCTOCODE_MCP.md`](OCTOCODE_MCP.md) for server lifecycle, and [`SECURITY.md`](SECURITY.md) for path, secret, and command boundaries.

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
  - [Cache storage and lifecycle](#cache-storage-and-lifecycle)
  - [npm registries and authentication](#npm-registries-and-authentication)
  - [`.env` — third-party API keys](#env--third-party-api-keys)
  - [`.octocoderc` — Octocode settings](#octocoderc--octocode-settings)
  - [How settings override each other](#how-settings-override-each-other)
- [MCP client `env` block](#mcp-client-env-block)
- [All settings reference](#all-settings-reference)
  - [Third-party keys](#third-party-keys)
  - [Octocode settings — env var or `~/.octocode/.octocoderc`](#octocode-settings--env-var-or-octocodeoctocoderc)
  - [Advanced runtime — env var only](#advanced-runtime--env-var-only)
  - [Protected keys](#protected-keys--never-sourced-from-env)
- [GitHub Enterprise](#github-enterprise)
- [Troubleshooting](#troubleshooting)
- [See also](#see-also)

---

## Quick setup

```bash
npx octocode auth login                          # authenticate (browser, encrypted token)
echo 'TAVILY_API_KEY=tvly-...' >> ~/.octocode/.env  # optional: add web search
npx octocode auth --json                         # verify
```

Already have a GitHub token and don't want a browser login? See [Method 2 — Token env var](#method-2--token-env-var).

---

## Authentication

Octocode needs a GitHub token to search code, read files, and call the GitHub API. Pick one of three ways to provide one.

### Method 1 — Octocode OAuth login (recommended)

Best for local use whenever a browser is available.

```bash
npx octocode auth login          # OAuth device flow; token saved to the OS credential store
npx octocode auth login --force  # replace an existing stored token
npx octocode auth logout         # delete the stored token
```

GitHub App tokens auto-refresh; standard `ghp_*` tokens don't expire. Octocode reads the stored token automatically on every request.

### Method 2 — Token env var

Best for CI/CD, MCP clients, and scripts. Set any one token var in your shell, CI, or MCP client `env` block (see [Token priority order](#token-priority-order)):

```bash
export OCTOCODE_TOKEN=ghp_...   # Octocode-specific, highest priority
export GITHUB_TOKEN=ghp_...     # or any of the standard vars
```

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"],
      "env": { "GITHUB_TOKEN": "ghp_..." }
    }
  }
}
```

Changes take effect on the next request — no restart needed. Token vars **cannot** go in `~/.octocode/.env`; they are [protected keys](#protected-keys--never-sourced-from-env) and the loader skips them. Use your shell, shell profile, or the MCP `env` block.

### Method 3 — gh CLI passthrough

Best if you already use the [GitHub CLI (`gh`)](https://cli.github.com/). After `gh auth login`, Octocode calls `gh auth token` as a fallback when it finds no other token.

### Token priority order

Octocode checks these in order and stops at the first non-empty value. **Env vars always beat stored credentials.**

| # | Type | Source | How to set |
|---|------|--------|-----------|
| 1 | Env var | `OCTOCODE_TOKEN` | `export OCTOCODE_TOKEN=ghp_...` |
| 2 | Env var | `GH_TOKEN` | `export GH_TOKEN=ghp_...` |
| 3 | Env var | `GITHUB_TOKEN` | `export GITHUB_TOKEN=ghp_...` · auto-set in GitHub Actions |
| 4 | Env var | `GITHUB_PERSONAL_ACCESS_TOKEN` | `export GITHUB_PERSONAL_ACCESS_TOKEN=ghp_...` |
| 5 | Octocode OAuth | encrypted storage | `npx octocode auth login` |
| 6 | gh CLI | `gh auth token` | `gh auth login` |

### Auth commands

```bash
npx octocode auth login [--force] [--hostname github.mycompany.com]  # OAuth (Enterprise via --hostname)
npx octocode auth logout        # delete the stored token
npx octocode auth [--json]      # show token source + GitHub username
npx octocode scheme             # tool catalog with availability
npx octocode config             # config file paths + which keys are set
```

---

## Config files

### Where everything lives

All Octocode config, credentials, cache, and session data live under the **Octocode home**: `.octocode` inside the OS home — `~/.octocode` (macOS/Linux) or `%USERPROFILE%\.octocode` (Windows). `XDG_CONFIG_HOME` and `APPDATA` are not used. Override the location for all products with `export OCTOCODE_HOME=/custom/path`.

| Path | What it does |
|------|-------------|
| `.env` | Third-party API keys (Tavily, Serper, …). Loaded by agents and skills, not the MCP server/CLI. |
| `.octocoderc` | Octocode behavior settings (tools, network, paths, output, storage). Read by the MCP server and CLI. |
| `stats.json` | Usage counters. Written only when `OCTOCODE_ENABLE_STATS=1`. |
| `session.json` | Session identity. |
| `tmp/clone/` | Git clones, keyed by owner, repo, and branch/sparse identity. |
| `tmp/tree/` | API-materialized files/directories, keyed by owner, repo, and immutable commit SHA. |
| `tmp/response/` | Shared file-backed L2 cache for eligible GitHub and package responses. |

### Cache storage and lifecycle

The CLI and MCP share the same file-based cache roots under `OCTOCODE_HOME` (no database, no editor-local storage).

Set `storage.mode` to `"memory"` when Octocode must not create persistent caches or session state: it disables clone/directory/exact-file materialization, response-cache disk reads and writes, cache maintenance, session and stats writes, and the Pi extension's SQLite state (kept in process memory until exit). File-content requests still return content without a `localPath`. It does not delete existing files or configured credentials.

| Concern | Behavior |
|---------|----------|
| Automatic cleanup | One full maintenance pass per 24-hour window across processes, guarded by a persisted due marker and a cross-process lock. |
| CLI | Due-check runs once per process on entering the execution runtime; no background timer. Help, schema, and context-only paths are side-effect free. |
| MCP | Server init runs the same due-check, then schedules the next deadline with an `unref()` timer, cancelled on shutdown. |
| Expiry | Clone/tree entries use the clone TTL (24 h default). Response entries use their own stale deadlines. Maintenance recovers stale clone temp artifacts and locks. |
| Failure | Best-effort; an unavailable or read-only cache home never blocks CLI execution or MCP startup. |
| Ownership | Traverses only Octocode-owned `clone`, `tree`, clone-artifact, and `response` roots; preserves unrelated `tmp/` directories. |
| Manual | `octocode cache status` prints the cache home and recent evictions; `octocode cache clear` deletes cached GitHub responses (remove `tmp/` checkouts directly). |

Response freshness depends on the result type (the 24-hour interval is a cleanup gate, not a universal freshness period):

| Response type | Freshness |
|---------------|-----------|
| File content | 5 minutes |
| GitHub user identity | 15 minutes |
| Pull requests, issues, and commit history | 30 minutes |
| Code search and releases | 1 hour |
| Repository search and repository structure | 2 hours |
| npm search | 4 hours |
| Unclassified response | 24 hours |

Conditional file/structure requests keep a stale body and ETag for up to 24 hours so a `304 Not Modified` restores the body without a full re-download. In-memory provider, client, branch-resolution, credential, and session caches are process-local and disappear on exit.

### npm registries and authentication

`artifactSearch` with `type:"npm"` loads npm configuration per query: environment, project/user `.npmrc`, global config, and npm defaults. `NPM_CONFIG_REGISTRY` selects the default registry (default `https://registry.npmjs.org/`); `NPM_CONFIG_USERCONFIG` selects the config file (lowercase equivalents supported). Exact `@scope/package` queries honor `@scope:registry`; the tool's optional `registry` field overrides routing for one query and its continuations.

Keep credentials in registry-scoped npm configuration — a token variable alone (e.g. `${NPM_TOKEN}`) is not associated with a registry. Do not put tokens in tool arguments or registry URLs. Results cached under one configuration identity are not reused under another.

### `.env` — third-party API keys

A plain `KEY=VALUE` file for third-party API keys used by web search and installed skills — **not** for Octocode's own settings.

**Where:** `~/.octocode/.env` (global) · `<project>/.octocode/.env` (project, overrides global, trusted projects only)

```bash
# ~/.octocode/.env
TAVILY_API_KEY=tvly-...   # curated, deeper research — https://app.tavily.com/
SERPER_API_KEY=...        # broad Google SERP results — https://serper.dev/
EXA_API_KEY=...           # neural/category-filtered search — https://dashboard.exa.ai/
```

Rules:
- A key set in your shell wins over this file.
- **Agent sessions and skill scripts load this file automatically; the MCP server and CLI do not** — pass those keys via your shell or the MCP `env` block.
- GitHub token vars are blocked here (see [protected keys](#protected-keys--never-sourced-from-env)).

Skills query every web-search engine whose key is set and validated, then fuse results (Serper, Tavily, and Exa are not interchangeable). With no key set, skills fall back to keyless DuckDuckGo.

### `.octocoderc` — Octocode settings

A JSONC file for Octocode's own behavior — tool availability, network, local path restrictions, output format, LSP config, and storage. Located at `~/.octocode/.octocoderc`. **Restart the MCP server or start a new agent session after editing.**

Every setting also has an **env var**, and env vars always win. See [Octocode configuration settings](generated/CONFIG_SETTINGS.md) for the complete generated example, env mappings, defaults, constraints, credential policy, and token priority (generated from the same contract consumed by TypeScript and Rust).

Unknown keys emit a warning with their full path, so a misspelling like `local.enableLocl` is visible instead of silently defaulting. `tools.enabled` / `TOOLS_TO_RUN` is a strict allowlist (e.g. `["ghSearch","localSearch"]`); `tools.disabled` / `DISABLE_TOOLS` removes names from the default set. An allowlist cannot bypass availability policy — MCP still omits `clasify` unless `OCTOCODE_CLASSIFICATION_API` is nonblank.

### How settings override each other

```
Shell env vars / MCP client env block       ← always win, highest priority
<project>/.octocode/.env → ~/.octocode/.env ← API keys (agent/skills only)
~/.octocode/.octocoderc                     ← Octocode settings (MCP server + CLI)
Built-in defaults                           ← lowest
```

- **Env vars always beat file config.**
- **`.env` is only for agent/skill sessions** — the MCP server and CLI don't load it.
- **GitHub tokens never come from `.env`** — blocked there regardless of priority.

---

## MCP client `env` block

Configure the MCP server without a shell profile by passing env vars in your client config:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_...",
        "REQUEST_TIMEOUT": "60000",
        "GITHUB_API_URL": "https://ghe.mycompany.com/api/v3"
      }
    }
  }
}
```

Run `npx octocode install --ide cursor` to write this automatically (`--ide` also accepts `vscode`, `claude`, `windsurf`, and others).

---

## All settings reference

### Third-party keys

Set in `~/.octocode/.env` or your shell. Skills read them; the MCP server and CLI do not.

| Key | Default | Notes |
|-----|---------|-------|
| `TAVILY_API_KEY` | unset | Web search — curated research. [Get a key](https://app.tavily.com/) |
| `SERPER_API_KEY` | unset | Web search — Google SERP results. [Get a key](https://serper.dev/) |
| `EXA_API_KEY` | unset | Web search — neural/category search. [Get a key](https://dashboard.exa.ai/) |

### Octocode settings — env var or `~/.octocode/.octocoderc`

Settings tables, defaults, ranges, enum values, aliases, protected-environment policy, GitHub token priority, and `OCTOCODE_HOME` behavior are generated in [Octocode configuration settings](generated/CONFIG_SETTINGS.md). Edit `packages/octocode-config/config-contract.json`, not this guide, when policy changes.

### Advanced runtime — env var only

Stats persistence (`OCTOCODE_ENABLE_STATS`) and the ghCloneRepo cache (`cloneCache.ttl` / `OCTOCODE_CACHE_TTL_MS`, `cloneCache.maxSize` / `OCTOCODE_MAX_CACHE_SIZE`, `cloneCache.maxClones` / `OCTOCODE_MAX_CLONES`) are contract settings: defaults, ranges, and `.octocoderc` support are in [generated settings](generated/CONFIG_SETTINGS.md). `storage.mode="memory"` overrides settings that otherwise enable disk caching or stats.

Response-cache entry counts/sizes and per-surface tool-call timeouts are bounded internally and not configurable via env vars (CLI uses a longer window for LSP cold starts). Network timeout/retries use `REQUEST_TIMEOUT` / `MAX_RETRIES`. Classification provider requests (`clasify`) share one process-wide gate per provider endpoint: at most `OCTOCODE_CLASSIFICATION_CONCURRENCY` (`classification.maxConcurrency`, default `10`, range 1–64) in flight, one tool call may use up to three quarters of it, and the gate halves itself on provider throttling (429/503/529, `Retry-After` pauses every caller) and recovers gradually. Use `octocode cache clear` / `octocode cache status` for the GitHub content cache.

### Protected keys — never sourced from `.env`

Octocode always ignores these when loading any `.env`, whatever their values. Set them in your shell, CI, or the MCP `env` block.

| Key | Why protected |
|-----|---------------|
| `OCTOCODE_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`, `GITHUB_PERSONAL_ACCESS_TOKEN` | GitHub auth — must be explicit |
| `PATH` | OS binary resolution — `.env` must not hijack it |
| `HOME` | OS home directory |
| `SHELL` | Login shell |
| `USER` / `LOGNAME` | User identity |
| `PWD` | Working directory |
| `TMPDIR` | System temp directory |
| `NODE_OPTIONS` | Node runtime flags — a security risk if `.env` could set them |
| `PYTHON` | Python interpreter path |
| `GITHUB_API_URL` | GitHub API root — set via shell or `.octocoderc` (`github.apiUrl`), never `.env`, so an untrusted project can't redirect API traffic |
| `OCTOCODE_CLASSIFICATION_API` (jev alias `OCTOCODE_JEV_KEY`) | Classification credential — shell, `.octocoderc` (`classification.api`), or the trusted home `.env`; never a project `.env`. Excluded from resolved config. Set it to an empty string in the process env (`OCTOCODE_CLASSIFICATION_API=`) to disable `clasify` for that process; no file or vendor-key fallback applies |
| `OCTOCODE_CLASSIFICATION_API_HOST` | Vendor API-root override — controls where the key is sent; same placement rules as the key |
| `OCTOCODE_CLASSIFICATION_TYPE` | Vendor selector (`classification.type`, default `jev`) |

---

## GitHub Enterprise

```bash
export GITHUB_TOKEN="ghp_your_ghe_token"
export GITHUB_API_URL="https://github.mycompany.com/api/v3"
export OCTOCODE_GITHUB_CLIENT_ID="your_oauth_app_client_id"  # required for GHE OAuth device login/refresh
npx octocode auth login --hostname github.mycompany.com
```

Or set it permanently in `~/.octocode/.octocoderc`:

```jsonc
{ "github": { "apiUrl": "https://github.mycompany.com/api/v3" } }
```

---

## Troubleshooting

Always start with `npx octocode auth --json` (token source + identity), `npx octocode scheme` (tool availability), and `npx octocode config` (config paths + which keys are set).

| Symptom | Fix |
|---------|-----|
| No token / 401 | `npx octocode auth login`, or set `GITHUB_TOKEN` in shell or MCP `env` block |
| Wrong GitHub account | `npx octocode auth logout` then `auth login` — or `auth login --force` |
| Env token overriding saved token | Env always wins — unset the env var |
| `ghCloneRepo` unavailable | Use the CLI with `OCTOCODE_STORAGE_MODE=persistent`. MCP never exposes cloning. Check `npx octocode scheme`. |
| `astRewrite` unavailable | It's a beta feature: set `OCTOCODE_BETA=true` or `local.beta: true` (the sole gate for both preview and apply). MCP registers it only when this gate and local tools are enabled. |
| Local tools turned off | Ensure neither `ENABLE_LOCAL` nor `local.enabled` is `false` |
| A tool is missing | Inspect `npx octocode scheme`; check `TOOLS_TO_RUN` / `tools.enabled` (strict allowlists) and `DISABLE_TOOLS` / `tools.disabled`. Removed tool names are not aliases. |
| Slow / timeouts | Raise `REQUEST_TIMEOUT` (max `300000` ms) |
| `clasify` returns `classificationRateLimited` | The provider is throttling; wait for the reported retry time, or lower `OCTOCODE_CLASSIFICATION_CONCURRENCY` when several agents share one key |
| A skill's external search is unavailable | Follow that skill's provider/credential instructions; the catalog exposes no general web-search tool. |
| `stats.json` never written | Set `OCTOCODE_ENABLE_STATS=1` (off by default) |
| `.env` key ignored | Token vars are blocked in `.env` — use your shell or the MCP `env` block |
| `.env` key not loading | Confirm the agent session restarted and the project is trusted |
| Enterprise hitting github.com | Set `GITHUB_API_URL` in both shell and `.octocoderc` |
| Enterprise device login/refresh rejected | Set `OCTOCODE_GITHUB_CLIENT_ID` to an OAuth app client ID registered on that GHE host |
| Settings not taking effect | Restart the MCP server or start a new agent session after editing `.octocoderc` |

---

## See also

- [Adding config to Octocode](ADDING_CONFIG.md) — contributor guide for adding settings, sections, and credentials
- [Octocode tools reference](OCTOCODE_TOOLS.md) — all tools and parameters
- [Octocode MCP server](OCTOCODE_MCP.md) — startup lifecycle and client config
- [Octocode CLI guide](../packages/octocode/docs/OCTOCODE_CLI.md) — all CLI commands
- [LSP server lifecycle](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md) — custom language server config
- [Security](SECURITY.md) — secret redaction and path validation
