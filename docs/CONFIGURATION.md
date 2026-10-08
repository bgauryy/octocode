# Octocode configuration

Configures feature gates, storage, caches, timeouts, config files, and environment precedence for the Octocode toolkit. Configuration controls whether a capability is available; it does not change a tool's input schema. Every setting, its env var, default, and range: [generated settings](generated/CONFIG_SETTINGS.md). Credentials: [AUTHENTICATION.md](AUTHENTICATION.md). Tool fields: [OCTOCODE_TOOLS.md](OCTOCODE_TOOLS.md). Server lifecycle: [OCTOCODE_MCP.md](OCTOCODE_MCP.md). Path, secret, and command boundaries: [SECURITY.md](SECURITY.md).

## Quick setup

```bash
npx octocode auth login            # GitHub OAuth device flow; encrypted token in ~/.octocode
npx -y octocode config view        # settings, keys, and agent MCP entries
npx octocode auth status --json    # verify
```

To skip the browser login, set `GITHUB_TOKEN`; a token env var always beats a stored login. Token order, login, refresh, Enterprise, the `clasify` key, and npm registry credentials: [AUTHENTICATION.md](AUTHENTICATION.md).

## Where everything lives

The **Octocode home** is one folder shared by the CLI, the MCP server, the VS Code extension, and installed skills: `~/.octocode` on macOS and Linux, `%USERPROFILE%\.octocode` on Windows. `XDG_CONFIG_HOME` and `APPDATA` are not used. To move it, set `OCTOCODE_HOME=/custom/path` for every product. Deleting `tmp/` is always safe. Deleting `credentials.json` and `.key` signs you out.

| Path in the home | Owner | What it holds |
|---|---|---|
| `.octocoderc` | you | Global Octocode settings. See [`.octocoderc`](#octocoderc--octocode-settings). |
| `.env` | you | Trusted environment fallbacks: tokens, the classification key, third-party keys. Only this file may set [home-only keys](#env--environment-fallback). |
| `credentials.json`, `.key` | Octocode | The encrypted GitHub login and its key. `auth logout` removes both. |
| `.credentials.lock` | Octocode | Serializes credential writes between processes. |
| `skills/` | Octocode | Skills installed with `octocode skill install`; hosts such as Claude, Cursor, or Codex link to these copies. |
| `stats.json` | Octocode | Clasify calls and tokens. Written only when `storage.stats` (`OCTOCODE_ENABLE_STATS=1`) is on and storage is persistent. |
| `logs/evictions.jsonl` | Octocode | Cached checkouts that cleanup removed; `octocode cache status` shows recent ones. |
| `tmp/response/` | cache | GitHub file bodies and directory listings, shared by CLI and MCP to save rate limit. |
| `tmp/clone/` | cache | `ghCloneRepo` (CLI) checkouts, keyed by owner, repo, and branch or sparse selection; may hold local edits. |
| `tmp/materialize/v2/` | cache | Files `ghStructure` materialized at an immutable commit SHA. A manifest beside each commit directory lists the files; a directory without its manifest is cleared, never reused. |
| `tmp/ratelimit/` | cache | Last-known GitHub rate-limit state, shared across processes. |
| `tmp/clone-locks/`, `tmp/clone-tmp/`, `tmp/git-home/` | cache | Per-repository clone locks, staging, and an isolated git home. |

Clone git runs without your git config. It keeps `PATH`, the proxy variables (`HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`, `NO_PROXY`, either case), and CA bundles (`SSL_CERT_FILE`, `SSL_CERT_DIR`, `GIT_SSL_CAINFO`, `GIT_SSL_CAPATH`, `CURL_CA_BUNDLE`) from the runtime environment; a project `.env` cannot set them.

A skill without a project writes reports and state to a named home folder, for example `research/` or `agents-communication/`. Inside a project, skills use `<project>/.octocode/`, which also holds the workspace `.octocoderc` and `.env` and `graph/` (the stored code graph).

### Cache storage and lifecycle

CLI and MCP share the file-based cache roots under `OCTOCODE_HOME`; there is no database or editor-local storage.

`storage.mode: "memory"` stops persistent caches and session state: no clone, directory, or exact-file materialization, no response-cache disk reads or writes, no cache maintenance, and no stats writes. File-content requests still return content, without a `localPath`. It does not delete existing files or credentials.

| Concern | Behavior |
|---|---|
| Cleanup | At most one sweep per 24 hours (marker `tmp/.last-cache-maintenance`). It deletes entries older than 24 hours under `tmp/response`, `tmp/materialize/v2`, the legacy `tmp/tree`, and `tmp/ratelimit`, and search snapshots older than 60 seconds. Clones use lock- and status-aware eviction during clone activity; age sweeps preserve them. |
| When | Synchronously at native runtime start: once per CLI process and at MCP server start. No background timer. Skipped in `memory` mode or when `tmp/` does not exist. |
| Failure | Best-effort; an unavailable or read-only cache home never blocks the CLI or MCP startup. |
| Ownership | Only Octocode-owned directories; other `tmp/` content stays. |
| Manual | `octocode cache status` prints the cache home and recent evictions; `octocode cache clear` deletes cached GitHub files and listings. Remove `tmp/` checkouts directly. |

GitHub responses are cached in memory (4,096 entries, 64 MiB) and, with persistent storage, in `tmp/response/`. File bodies and directory listings (`ghGetFileContent`, the listings `ghStructure` reads) are fresh for 60 seconds, then revalidated by ETag; bodies read at a commit SHA are never revalidated. Search pages (`ghSearchCode`, `ghSearchRepo`, and the `ghSearchHistory` searches) are served from cache for 60 seconds with no request or spacing wait; a page GitHub marks incomplete is never stored. REST history reads (pull-request and commit lists, `ghGetHistoryItem`) send a conditional request every call, so a `304` skips the body. Package calls keep per-process memos. Provider, client, branch, credential, and session caches are process-local.

### `.env` — environment fallback

A plain `KEY=VALUE` file: `~/.octocode/.env` (global) and `<project>/.octocode/.env` (workspace, overrides global).

```bash
# ~/.octocode/.env
GH_TOKEN=ghp_example
OCTOCODE_CLASSIFICATION_API=classification_key_example
TAVILY_API_KEY=tvly-...   # curated research search — https://app.tavily.com/
SERPER_API_KEY=...        # Google SERP results — https://serper.dev/
EXA_API_KEY=...           # neural/category search — https://dashboard.exa.ai/
```

- A non-empty key in the shell or MCP client environment wins over both files. The workspace file wins over the global file; missing or blank workspace values fall back to global.
- CLI and MCP load both files. Node helpers load the supplied workspace by default; an embedding host can opt out with `trusted: false`. Native `trustedProject` controls executable language-server configuration, not dotenv loading.
- **Home-only keys** are skipped in a workspace `.env` and listed under `skippedProtected` in `octocode config --json`: `GITHUB_API_URL`, `OCTOCODE_BETA`, `ALLOWED_PATHS`, `WORKSPACE_ROOT`, `OCTOCODE_ALLOW_PRIVATE_REGISTRY`, `OCTOCODE_LSP_CONFIG`, `OCTOCODE_LSP_AUTO_INSTALL`, `OCTOCODE_LSP_CACHE_DIR`, `OCTOCODE_TRUST_PROJECT_LSP_CONFIG`, `OCTOCODE_STORAGE_MODE`, `OCTOCODE_CLASSIFICATION_API_HOST`, and `OCTOCODE_CARGO`. GitHub tokens and the classification key are accepted in either file.
- `OCTOCODE_STORAGE_MODE` (and `storage.mode`) from a workspace may only be `memory`, which opts that project out of persistence; a workspace `persistent` value is ignored with a `workspace_config_protected` warning.
- Empty values normally allow fallback. A present-but-blank `OCTOCODE_CLASSIFICATION_API` disables classification and blocks file fallback.
- Both files always block these OS and runtime keys; set them in the shell, CI, or the MCP `env` block: `PATH`, `HOME`, `SHELL`, `USER`, `LOGNAME`, `PWD`, `TMPDIR`, `NODE_OPTIONS`, `PYTHON`, `OCTOCODE_HOME`.
- Credential aliases resolve by source first, then by [alias priority](AUTHENTICATION.md#token-environment-variables). Metadata shows names and source files, never values. Keep credential files out of version control.

Skills query every web-search engine whose key is set and valid, then fuse results (Serper, Tavily, and Exa are not interchangeable). With no key, skills fall back to keyless DuckDuckGo.

### `.octocoderc` — Octocode settings

A JSONC file for Octocode's own behavior: `~/.octocode/.octocoderc` (or `$OCTOCODE_HOME/.octocoderc`) and `<project>/.octocode/.octocoderc`, where `<project>` is the process working directory. **Restart the MCP server or start a new agent session after editing.**

```jsonc
// <project>/.octocode/.octocoderc — only what this repo changes
{
  "output": { "format": "json" },
  "tools": { "disabled": ["ghSearchRepo"] }
}
```

- Layering is per field: a workspace value replaces only that field. Arrays replace, never concatenate; `null` on a list field (for example `tools.enabled`) resets it.
- Both files rank below every environment source, including the global `.env`. Every setting has an env var; see [generated settings](generated/CONFIG_SETTINGS.md).
- The workspace file loads without a trust flag, like the workspace `.env`. A field bound to a home-only key (for example `local.allowedPaths`, `local.beta`, `github.apiUrl`) is ignored there, with a warning.
- When the working directory is your OS home, `<project>/.octocode` is the Octocode home, and the file is read once.
- An unknown key warns with its full path, so a typo like `local.enableLocl` is visible.
- `tools.enabled` / `TOOLS_TO_RUN` is a strict allowlist (e.g. `["ghSearchCode","localSearch"]`); `tools.disabled` / `DISABLE_TOOLS` removes names from the default set. Unknown or removed tool names are ignored without a warning. An allowlist cannot bypass availability: MCP still omits `clasify` without a [classification key](AUTHENTICATION.md#classification-key-clasify).

### How settings override each other

```
Shell env vars / MCP client env block       ← highest
<workspace>/.octocode/.env
~/.octocode/.env
<workspace>/.octocode/.octocoderc
~/.octocode/.octocoderc
Built-in defaults                           ← lowest
```

Precedence is per field: the first source with a valid value wins. Env vars always beat file config. At each tier, workspace beats home. Missing, blank, or invalid values fall back to the next source. GitHub and classification credentials use the same order; token discovery and OAuth refresh stay in native.

### Misconfiguration never blocks startup

A bad value is skipped and reported once per process on **stderr** (stdout carries CLI results and MCP JSON-RPC), with the file or variable and the reason:

```
octocode: config warning: /repo/.octocode/.octocoderc: network.maxRetries: Must be a number; value ignored [invalid_config]
octocode: config warning: /repo/.octocode/.octocoderc: Failed to parse config file: …; the whole file is ignored [config_load_error]
octocode: config warning: /Users/me/.octocode/.env: REQUEST_TIMEOUT is not a valid integer for network.timeout; value ignored [invalid_env_value]
```

| Problem | Effect |
|---|---|
| Unreadable file, invalid JSON, or non-object root | That file is ignored; other layers apply. |
| Invalid field (type, range, relative path, non-http URL, unknown enum) | That field falls back to the next layer or the default. |
| A section that is not an object (`"network": 5`) | That section is dropped. |
| Unknown key | Warning only. |
| Invalid nonblank env value | Skipped, without printing the value. |

Warnings print on tool calls and `octocode config`, not on `octocode schema`. `octocode config` lists both `.octocoderc` files and their top-level keys; `--json` adds the `diagnostics` array.

## Local configuration view

`npx -y octocode config view` opens a temporary loopback page that edits home or workspace settings, keys, and the Octocode entries in agent configs. `--no-open` prints the session link instead. `--idle-timeout <seconds>` (default 900, 30–3600) closes the server after inactivity. Ctrl+C or **End session** stops it.

**Save to** picks the scope. **Home** writes `~/.octocode/.octocoderc` and `.env` for every project. **Workspace** writes `<project>/.octocode/.octocoderc` and `.env`, where `<project>` is the directory where you ran the command; it overrides home. Each save writes immediately and reloads the page data.

- **Settings:** search by key (`output.format`). Booleans are checkboxes, choices are lists, numbers must be in range, lists are JSON string arrays (`["localSearch","localFetch"]`, or `null` to inherit), text and paths are plain text. **Save** writes the scope; **Use inherited value** removes it from that scope only. **Effective value** and **Effective source** show what applies and from which file. If a home save does not change the effective value, look for a workspace or env override. Home-only settings (`local.allowedPaths`, `local.beta`, `github.apiUrl`, `lsp.configPath`) are disabled in Workspace scope, and `storage.mode` accepts only `memory` there.
- **Keys:** enter a **Key name** (letters, digits, `_`; not starting with a digit) and a one-line **New value**, then **Save key**. The key shows as **Set**; the value is never shown again. **Replace** and **Remove** edit it. Workspace scope refuses home-only keys (`ALLOWED_PATHS`, `WORKSPACE_ROOT`, `GITHUB_API_URL`, `OCTOCODE_BETA`, `OCTOCODE_LSP_CONFIG`, …) but accepts GitHub tokens and `OCTOCODE_CLASSIFICATION_API`. A credential stored in `.octocoderc` (`classification.api`) shows **Stored setting**.
- **Agents:** one card per MCP client file, for example Cursor `~/.cursor/mcp.json` or Claude Code `~/.claude.json`, with scope `home`, `workspace`, or `local`. **Install** (with **Launch with** `npx`, `bunx`, or `pnpm`) adds the entry; **Save agent key** / **Remove key** edit its env; clearing **Enabled** and **Save / update** turns it off; **Remove Octocode** deletes only the Octocode entry. Restart the agent after an edit; client env values can override the shared Octocode files.

Agent discovery checks supported client paths and the current workspace, not other projects or enterprise policies, and reports configuration status, not a live connection. Edits keep other MCP entries. Unsupported or ambiguous layouts, Continue main configuration arrays, symbolic links, and unreadable or malformed files are read only, with configuration warnings; imported Continue blocks report unknown coverage.

| Save error | Fix |
|---|---|
| *Configuration changed* / *Agent config changed* | Another process edited the file; the page reloads it. Review and save again. |
| *Invalid value for …* | Wrong type, out of range, or not an allowed choice. |
| *may only be configured in home scope* / *cannot be loaded from this .env scope* | Select **Home**, or set the variable in the launching environment. |
| *Values must be a single line* | Remove line breaks. |

Octocode's `.env` and `.octocoderc` are replaced without backup, so a removed secret leaves no copy. An agent client file keeps one rolling hidden backup beside it (`~/.cursor/.mcp.json.bak`); `octocode install --rollback <backup>` restores it.

The page uses bundled assets and an authenticated server on `127.0.0.1` at a temporary port. It checks request host and origin and grants no cross-origin access. The printed link holds a one-use session credential; keep it private. A browser extension that can read the page can read the values you enter. Keys in `.env` and credential settings (`classification.api`, `classification.apiHost`) in `.octocoderc` are not encrypted; on Unix, new config files and backups are owner-only. The view does not touch the GitHub login storage. Keep workspace key files out of version control. For terminal entry without shell history, use `octocode config set KEY --stdin`.

## MCP client `env` block

Any env var can go in the client config instead of a shell profile:

```json
{"mcpServers":{"octocode":{"command":"npx","args":["-y","octocode-mcp@latest"],"env":{"GITHUB_TOKEN":"ghp_...","REQUEST_TIMEOUT":"60000"}}}}
```

`npx octocode install --ide cursor` writes this entry; `install --list` prints every client id ([CLI `install`](../packages/octocode/docs/OCTOCODE_CLI.md#install--mcp-client-setup)).

## All settings reference

Defaults, ranges, enums, and env mappings: [generated settings](generated/CONFIG_SETTINGS.md). Notes that the table does not carry:

- **Storage:** `storage.stats` and the `ghCloneRepo` cache (`storage.cloneCache.ttl`, `.maxSize`, `.maxClones`) are overridden by `storage.mode: "memory"`.
- **Language servers:** `~/.octocode/lsp-servers.json` (or the `lsp.configPath` / `OCTOCODE_LSP_CONFIG` file) maps an extension to a server, replacing the built-in one: `{"languageServers":{".ts":{"command":"tsgo","args":["--lsp","-stdio"],"languageId":"typescript"}}}`. Assembly servers come only from this file. `lsp.trustProjectConfig` also trusts a project `.octocode/lsp-servers.json`; `lsp.autoInstall` governs `octocode lsp-server install`; `lsp.cacheDir` moves managed installs; `lsp.prewarm` controls background starts. See [LSP server lifecycle](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md).
- **Fixed internally:** response-cache sizes and per-surface tool-call timeouts (the CLI allows longer for LSP cold starts). Network timeout and retries use `REQUEST_TIMEOUT` / `MAX_RETRIES`.
- **Classification gate:** one process-wide gate per provider endpoint allows `OCTOCODE_CLASSIFICATION_CONCURRENCY` (`classification.maxConcurrency`) requests in flight; one tool call may use up to three quarters. Throttling (429/503/529) halves the gate, `Retry-After` pauses every caller, and the gate recovers gradually.

## GitHub Enterprise

Set `GITHUB_API_URL` (or `github.apiUrl` in the home `.octocoderc`) to the enterprise API root, for example `https://github.mycompany.com/api/v3`. A workspace file cannot set it. Tokens and device login: [AUTHENTICATION.md](AUTHENTICATION.md#github-enterprise).

## Troubleshooting

Start with `npx octocode auth status --json` (token source, identity), `npx octocode schema` (enabled tools; `schema <name>` shows a disabled tool's gate), and `npx octocode config` (paths, set keys).

| Symptom | Fix |
|---|---|
| Token, login, Enterprise, or `clasify` key problem | [AUTHENTICATION.md troubleshooting](AUTHENTICATION.md#troubleshooting) |
| `ghCloneRepo` unavailable | Use the CLI with `OCTOCODE_STORAGE_MODE=persistent`; MCP never exposes cloning. |
| `astRewrite` or `astTopology` unavailable | Beta and CLI-only: set `OCTOCODE_BETA=true` or `local.beta: true` (shell or home config) and run `octocode <tool>`. MCP never registers them. |
| Local tools off | Neither `OCTOCODE_ENABLE_LOCAL` nor `local.enabled` may be `false`. |
| A tool is missing | `npx octocode schema <name>`; check `TOOLS_TO_RUN` / `tools.enabled` and `DISABLE_TOOLS` / `tools.disabled`. |
| Slow or timeouts | Raise `REQUEST_TIMEOUT` (max `300000` ms). |
| `clasify` returns `classificationQuotaExhausted` | The provider account has no credit (HTTP 402); add credit and rerun. |
| `clasify` returns `classificationRateLimited` | Wait for the reported retry time, or lower `OCTOCODE_CLASSIFICATION_CONCURRENCY` when agents share a key. |
| A skill's web search is unavailable | Follow that skill's provider instructions; the catalog has no general web-search tool. |
| `stats.json` never written | Set `storage.stats: true` or `OCTOCODE_ENABLE_STATS=1`. |
| `.env` key ignored | A home-only key in a workspace `.env`, or the shell or MCP `env` already sets it; see `octocode config --json` → `skippedProtected`. |
| `.env` key not loading | Check the process working directory, then restart. |
| Settings not applied | Restart the MCP server or agent session; check stderr or `config --json` → `diagnostics`; confirm the working directory is the workspace you edited. |

## See also

- [Adding config](../skills-dev/octocode-dev/docs/ADDING_CONFIG.md) — contributor guide for settings and credentials
- [Octocode CLI guide](../packages/octocode/docs/OCTOCODE_CLI.md)
