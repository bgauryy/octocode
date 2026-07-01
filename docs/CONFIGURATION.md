# Configuration Reference

Concise reference for Octocode environment variables, the `.env` file, `.octocoderc`, local state paths, and option precedence.

> **One config home for every Octocode surface.** The MCP server, the `octocode` CLI, the awareness memory store, and the `octocode-agent` / Pi extension all resolve the same per-platform **Octocode home** (macOS `~/.octocode`, Linux `${XDG_CONFIG_HOME:-~/.config}/.octocode`, Windows `%APPDATA%\.octocode`; override with `OCTOCODE_HOME`). Everything — `.env`, `.octocoderc`, credentials, LSP config, caches — lives under that single directory, with an optional per-project `.octocode/` for overrides. Each surface reads a different subset of these files; see [Config surfaces — who reads what](#config-surfaces--who-reads-what).

## Sources And Precedence

Octocode resolves configuration in this order:

```text
explicit environment variables
  > <octocode-home>/.env   (and project .octocode/.env — loaded by the agent + skills via @octocodeai/config)
  > <octocode-home>/.octocoderc
  > built-in defaults
```

- **Environment variables** — per-client/per-project settings and tokens. Highest priority.
- **`.env`** — environment variables (incl. tool API keys such as `TAVILY_API_KEY`). The `octocode-agent` / Pi-extension surface loads these into `process.env` at session start; an explicit env var already set always wins. (The standalone MCP server / CLI read env from your shell or client config; they do not auto-load `.env`.)
- **`.octocoderc`** — structured machine-wide defaults (no secrets).

Restart the MCP server (or start a new agent session) after changing any source.

### Config surfaces — who reads what

The **home is unified** (every surface resolves the same `<octocode-home>` and `OCTOCODE_HOME`), but each surface reads a different subset of files under it. This matrix is the source of truth:

| Surface | `.octocoderc` | `<home>/.env` + project `.octocode/.env` | Notes |
|---------|:---:|:---:|-------|
| **MCP server** | ✅ | ❌ | Env comes from the client `env` block (see below); does not auto-load `.env`. |
| **`octocode` CLI** | ✅ | ❌ | Env comes from your shell; does not auto-load `.env`. |
| **`octocode-agent` / Pi extension** | ❌ | ✅ | Loads `.env` into `process.env` at session start (global always; project when trusted). |
| **Skills** (e.g. brainstorming web search scripts) | ❌ | ✅ | Load `<home>/.env` (+ project) via the shared loader, then a skill-local `.env` as a standalone fallback; `process.env` always wins. |

**The single loader.** All env/config loading flows through one zero-dependency, cross-platform npm package: [`@octocodeai/config`](../packages/octocode-config). It resolves the home (`getOctocodeHome`), parses `.env` (global + project) with the protected-key + precedence rules (`propagateOctocodeEnv`), and reads `.octocoderc` (`loadOctocoderc`). Packages use it as a workspace dep at build time; the Pi extension inlines the source as `dist/env.js` and skill scripts receive a static copy (`octocode-config.mjs`) — both injected at build, so nothing needs `@octocodeai/config` at runtime and it does not have to be published. `scripts/octocode-env.mjs` is a thin re-export kept for backward compatibility. This is the one place to change env/config behavior. *(MCP server / CLI adoption — auto-loading `<home>/.env` via this same module — is the remaining step; today they take env from the client/shell and read `.octocoderc`.)*

Practical consequences:

- **API keys (Tavily/Serper) are unified in practice under the agent.** Put them once in `<home>/.env`; the agent propagates them into `process.env`, which the `web` tool and skill scripts both prefer over any skill-local `.env`.
- **Standalone MCP/CLI do not read `.env`.** Pass their env via the client config (MCP `env` block) or your shell. Structured settings for them go in `.octocoderc`.
- **Tokens** never live in `.octocoderc` or `.env` — use `npx octocode auth login` (encrypted `credentials.json`) or shell env.

> **Fully unifying `.env`** (having the MCP server and CLI also auto-load `<home>/.env`, and pointing skill scripts at `<home>/.env` directly) is a possible future change — today only the agent surface auto-loads it.

## MCP Env Example

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "@octocodeai/mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_xxxxxxxxxxxx",
        "ENABLE_CLONE": "false"
      }
    }
  }
}
```

Install helpers write client-specific paths automatically:

```bash
npx octocode install --ide cursor
```

Supported clients are listed in the [Octocode CLI Guide](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md#install).

## `.octocoderc`

Path:

```text
<octocode-home>/.octocoderc
```

The file is JSON with comments/trailing commas tolerated. Tokens do not belong here.

```jsonc
{
  "version": 1,
  "github": {
    "apiUrl": "https://api.github.com"
  },
  "local": {
    "enabled": true,
    "enableClone": false,
    "workspaceRoot": "/absolute/workspace",
    "allowedPaths": []
  },
  "tools": {
    "enabled": null,
    "enableAdditional": null,
    "disabled": null
  },
  "network": {
    "timeout": 30000,
    "maxRetries": 3
  },
  "lsp": {
    "configPath": null
  },
  "output": {
    "format": "yaml",
    "pagination": {
      "defaultCharLength": 20000
    }
  }
}
```

Invalid file values fall back to defaults or env overrides. Unknown keys warn and are ignored.

## `.env` — Environment File

Plain `KEY=VALUE` environment file for values that belong in `process.env` — most importantly **tool API keys** like `TAVILY_API_KEY` and `SERPER_API_KEY` for the `web` search tool. Two locations, project overrides global:

```text
<octocode-home>/.env      # global (macOS ~/.octocode/.env)
<project>/.octocode/.env  # project — wins when the project is trusted
```

The `octocode-agent` / Pi extension loads these at **session start** into `process.env`, so they reach the `web` tool, `bash`, hooks, the bundled `octocode` CLI, and skill scripts. Format: `KEY=VALUE`, `#` comments, optional `export ` prefix, surrounding quotes stripped, no shell expansion. A template ships as [`packages/octocode-pi-extension/.env.example`](../packages/octocode-pi-extension/.env.example).

```bash
# ~/.octocode/.env
TAVILY_API_KEY=tvly-...
SERPER_API_KEY=...
# OCTOCODE_WEB_USER_AGENT=Mozilla/5.0 ... Chrome/125.0.0.0 Safari/537.36
```

Rules and safety:

- **Precedence:** an explicit shell/client env var already set **wins** over the file; project `.env` overrides global for the same key.
- **Trust gate:** the project `.octocode/.env` is loaded only after the project is trusted; the global file is always loaded.
- **Protected keys are never overwritten:** `PATH`, `HOME`, `SHELL`, `USER`, `LOGNAME`, `PWD`, `TMPDIR`, `NODE_OPTIONS`, `PYTHON`, `OCTOCODE_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN` (skipped with a warning). **GitHub tokens do not go in `.env`** — use `npx octocode auth login` (encrypted `credentials.json`) or your shell/client env.
- **Not a secret vault:** the agent can read env via `bash: env`. Values are never logged (`octocode-status` reports key names + source file only).

## Options

| Env | `.octocoderc` | Default | Meaning |
|-----|---------------|---------|---------|
| `GITHUB_API_URL` | `github.apiUrl` | `https://api.github.com` | GitHub API endpoint. Use `/api/v3` for GitHub Enterprise. |
| `ENABLE_LOCAL` | `local.enabled` | `true` | Enable local filesystem and LSP tools. Set `false` to explicitly disable them. |
| `ENABLE_CLONE` | `local.enableClone` | CLI `true`, MCP `false` | Enable `ghCloneRepo` and directory fetch. The CLI respects an explicit `ENABLE_CLONE=false`; MCP clone tools require `ENABLE_CLONE=true`. |
| `WORKSPACE_ROOT` | `local.workspaceRoot` | `process.cwd()` | Root used for relative local paths and project context. Must be absolute when set. |
| `ALLOWED_PATHS` | `local.allowedPaths` | `[]` | Comma-separated env list or JSON array. Empty means unrestricted after path validation. |
| `TOOLS_TO_RUN` | `tools.enabled` | `null` | Strict whitelist. Overrides add/remove filters. |
| `ENABLE_TOOLS` | `tools.enableAdditional` | `null` | Add tools to the default enabled set. |
| `DISABLE_TOOLS` | `tools.disabled` | `null` | Remove tools from the default enabled set. |
| `REQUEST_TIMEOUT` | `network.timeout` | `30000` | Request timeout in ms. Clamped to `5000..300000`. |
| `MAX_RETRIES` | `network.maxRetries` | `3` | Retry attempts. Clamped to `0..10`. |
| `OCTOCODE_LSP_CONFIG` | `lsp.configPath` | unset | Custom LSP server config path. |
| `OCTOCODE_OUTPUT_FORMAT` | `output.format` | `yaml` | `yaml` or `json`. |
| `OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH` | `output.pagination.defaultCharLength` | `20000` | Auto-pagination character budget. Clamped to `1000..50000`. |

Env-only options:

| Env | Default | Meaning |
|-----|---------|---------|
| `OCTOCODE_TOKEN` | unset | Highest-priority GitHub token. |
| `GH_TOKEN` | unset | Second-priority GitHub token. |
| `GITHUB_TOKEN` | unset | Third-priority GitHub token. |
| `OCTOCODE_HOME` | platform default | Override the Octocode config home (the single directory holding `.env`, `.octocoderc`, credentials, caches). |
| `TAVILY_API_KEY` | unset | `web` tool search provider (Tavily — AI answer + results). First choice in the provider ladder. Set via `.env`. |
| `SERPER_API_KEY` | unset | `web` tool search provider (Serper — Google SERP). Used when no `TAVILY_API_KEY`. Set via `.env`. |
| `OCTOCODE_WEB_USER_AGENT` | Chrome UA | User-Agent for `web({url})` fetch and the DuckDuckGo fallback. Defaults to a mainstream browser UA. |
| `OCTOCODE_BULK_QUERY_TIMEOUT_MS` | `60000` | Per-query timeout inside a bulk tool call (ms). |
| `OCTOCODE_TOOL_TIMEOUT_MS` | `60000` | Outer timeout for the entire tool call (ms). |
| `OCTOCODE_COMMAND_CHECK_TIMEOUT_MS` | `5000` | System command availability check timeout (ms). |
| `OCTOCODE_CACHE_TTL_MS` | `86400000` | Tmp materialization cache TTL in ms. |
| `OCTOCODE_MAX_CACHE_SIZE` | `2147483648` | Tmp materialization cache size limit in bytes. |
| `OCTOCODE_MAX_CLONES` | `50` | Maximum cached clone/tree materialization count per tmp bucket. |

## Parsing Rules

| Type | Env format | `.octocoderc` format |
|------|------------|----------------------|
| Boolean | `"true"` / `"1"` / `"false"` / `"0"` | `true` / `false` |
| Number | Integer string | Number |
| List | Comma-separated string, such as `"a,b,c"` | JSON array |

## Important Interactions

- Auth tokens are env-only. Do not put tokens in `.octocoderc` **or `.env`** (both protect them) — use `npx octocode auth login` or shell/client env.
- **`.env` loads at session start** (agent surface): global `<octocode-home>/.env` always, project `<project>/.octocode/.env` only when trusted; project overrides global; already-set env wins; protected keys skipped. Propagates to the `web` tool, `bash`, hooks, the bundled `octocode` CLI, and skill scripts.
- **Web search provider ladder:** `TAVILY_API_KEY` → `SERPER_API_KEY` → DuckDuckGo (no key). The `web` tool auto-selects; check the active provider with `/octocode-status`.
- `TOOLS_TO_RUN` is a strict whitelist and overrides `ENABLE_TOOLS` and `DISABLE_TOOLS`.
- **Local tools default on.** Set `ENABLE_LOCAL=false` or `local.enabled:false` only when you want to disable the whole local surface. To hide individual tools, prefer `DISABLE_TOOLS` or `tools.disabled`.
- **Clone defaults differ by surface.** The CLI enables clone/materialization by default unless `ENABLE_CLONE=false`; MCP clone tools require `ENABLE_CLONE=true`.
- LSP requires local tools enabled. If `OCTOCODE_LSP_CONFIG` is unset, Octocode checks `<workspace>/.octocode/lsp-servers.json`, then `<octocode-home>/lsp-servers.json`.
- `WORKSPACE_ROOT` env overrides `local.workspaceRoot`.

## Local State

All state lives under Octocode home, a fixed per-platform directory (macOS `~/.octocode`, Linux `${XDG_CONFIG_HOME:-~/.config}/.octocode`, Windows `%APPDATA%\.octocode`):

| Path | Purpose |
|------|---------|
| `.octocoderc` | Persistent MCP config (env/local/tools/network/output). |
| `.env` | Environment vars (incl. `web` tool keys). Loaded into `process.env` by `octocode-agent` / Pi extension at session start. |
| `credentials.json` | Encrypted OAuth credentials. |
| `.key` | 32-byte AES key for credential encryption. |
| `session.json` | Session identity and timestamps. |
| `stats.json` | Usage counters and character savings. |
| `config.json` | CLI config (e.g. `skillsDestDir`). |
| `tmp/clone/` | Git clone and sparse-clone cache. |
| `tmp/tree/` | GitHub API file/tree materialization cache. |
| `tmp/binary/` | Text derived from binary/archive modes such as `extract`, `decompress`, and `strings`. |
| `tmp/unzip/` | Archive unpack output from `localBinaryInspect` / `unzip`. |
| `lsp-servers.json` | User-level LSP server config. |

Architecture details: [Credentials](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md#credential-architecture-api) · [Session](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md#session-persistence)

## Project `.octocode/` Folder

Octocode also reads a per-project `.octocode/` directory at the workspace root. It is not required — create it only when you need project-specific overrides.

| Path | Purpose |
|------|---------|
| `.octocode/lsp-servers.json` | Project-level LSP server config. Checked before `<octocode-home>/lsp-servers.json`. |
| `.octocode/.env` | Project environment vars (e.g. per-repo `web` tool keys). Overrides `<octocode-home>/.env`; loaded only when the project is trusted. |

### `lsp-servers.json` format

Keys are file-extension patterns (must start with `.`). `command` and `languageId` are
required; `args` (default `[]`) and `initializationOptions` (passed verbatim in the LSP
`initialize` request) are optional. A custom entry **overrides the built-in server** for that
extension, and — more usefully — **adds semantics for a language with no built-in server**
(e.g. Scala, Kotlin, Ruby):

```jsonc
{
  "languageServers": {
    // Bring-your-own: Scala has no built-in server — this gives it full semantics.
    ".scala": { "command": "metals", "args": ["stdio"], "languageId": "scala" },

    // Override a built-in: a different Java launch with init options.
    ".java": {
      "command": "jdtls",
      "args": ["-data", "/tmp/jdtls-workspace"],
      "languageId": "java",
      "initializationOptions": { "bundles": [] }
    }
  }
}
```

TypeScript/JavaScript are bundled — no entry needed. Set `OCTOCODE_LSP_CONFIG` to point to a
different file entirely. Without an entry, an unsupported extension's semantic ops throw
`lspServerUnavailable` and the agent falls back to text/structural search — see
[`LSP_SERVER_LIFECYCLE.md`](https://github.com/bgauryy/octocode/blob/main/docs/LSP_SERVER_LIFECYCLE.md#custom--bring-your-own-lsp-any-language).

## Quick Checks

```bash
echo "GITHUB_TOKEN: ${GITHUB_TOKEN:+set}"
echo "ENABLE_LOCAL: ${ENABLE_LOCAL:-not set}"
echo "ENABLE_CLONE: ${ENABLE_CLONE:-not set}"
echo "octocode home: ${OCTOCODE_HOME:-$HOME/.octocode}"
echo "web keys: TAVILY=${TAVILY_API_KEY:+set} SERPER=${SERPER_API_KEY:+set}"
npx octocode status --json
# In an octocode-agent / Pi session, /octocode-status reports the active web search provider.
```

Common fixes:

| Symptom | Check |
|---------|-------|
| Token missing | Set `OCTOCODE_TOKEN`, `GH_TOKEN`, or `GITHUB_TOKEN`, or run `npx octocode auth login`. Not via `.env` (protected). |
| Local tools unavailable | Check for `ENABLE_LOCAL=false`, `local.enabled:false`, or tool filters hiding them. |
| Clone unavailable (MCP) | Set `ENABLE_CLONE=true` and make sure local tools are not explicitly disabled. The CLI enables clone by default unless you set `ENABLE_CLONE=false`. |
| Tool hidden | Check `TOOLS_TO_RUN`, `ENABLE_TOOLS`, and `DISABLE_TOOLS`. |
| Timeout | Increase `REQUEST_TIMEOUT` up to `300000`. |
| Web search low quality / rate-limited | Add `TAVILY_API_KEY` or `SERPER_API_KEY` to `<octocode-home>/.env` (falls back to DuckDuckGo without a key). |
| `.env` key not applied | Confirm it is not a protected key, not already set in the shell, and (project file) the project is trusted. Start a new agent session after editing. |

## See Also

- [Authentication Setup](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md)
- [Octocode CLI Guide](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md)
- [Local Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#local-code-tools-reference)
- [LSP Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#lsp-tools-reference)
