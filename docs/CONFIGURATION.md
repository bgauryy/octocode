# Octocode configuration

Start here to configure Octocode for the CLI or an MCP client. This page owns tool availability, feature opt-ins, file precedence, storage and caches. The [generated settings reference](generated/CONFIG_SETTINGS.md) is the complete list of setting names, environment variables, defaults, types and ranges. Internal limits are [fixed](#fixed-internal-limits).

Related guides own [credentials and login](AUTHENTICATION.md), [tool inputs and workflows](OCTOCODE_TOOLS.md), [MCP client setup and lifecycle](OCTOCODE_MCP.md), [CLI commands](OCTOCODE_CLI.md), and [security boundaries](SECURITY.md).

## Quick setup

```bash
npx octocode auth login            # GitHub OAuth device flow; encrypted token in ~/.octocode
npx -y octocode config view        # settings, keys, and agent MCP entries
npx octocode auth status --json    # verify
```

To skip the browser login, set `GITHUB_TOKEN`; a token env var always beats a stored login. Token order, login, refresh, Enterprise, the `clasify` key, and npm registry credentials: [AUTHENTICATION.md](AUTHENTICATION.md).

## Tool availability: CLI and MCP

The CLI always lists and enables all 16 tool commands. With default settings and no classification key, MCP starts with 12 tools and exposes optional tools through the controls below. These controls choose the MCP catalog; they do not remove CLI commands or relax filesystem, credential, network or storage protections. A missing provider or language server fails the call that needs it.

| Tools | CLI catalog | MCP exposure | Requirements when called |
|---|---|---|---|
| Six GitHub search/read tools and `artifactSearch` | Always available | On by default | Provider access; credentials where the endpoint requires them |
| `localSearch`, `localFetch`, `structureSearch`, `astSearch`, `lspSearch` | Always available | On by default; `local.enabled` / `OCTOCODE_ENABLE_LOCAL` can disable them | Authorized paths; `lspSearch` also needs an appropriate language server |
| `graphBuild`, `graphQuery` | Always available | `local.beta: true` / `OCTOCODE_BETA=true`, with local tools enabled | Authorized scope and the graph operation's snapshot requirements |
| `clasify` | Always available | A configured, nonblank `OCTOCODE_CLASSIFICATION_API`, with `classification.api` as file fallback | A usable classification credential and provider |
| `ghCloneRepo` | Always available | `tools.enableClone: true` / `OCTOCODE_ENABLE_CLONE=true`; default `false` | Git, repository access and persistent storage |

`OCTOCODE_BETA` controls the graph tools only. It is not a classification alias. `clasify` visibility uses the configured key, with no startup network health check; a rejected key or provider outage is reported when a call runs. A present-but-blank `OCTOCODE_CLASSIFICATION_API` blocks fallback and disables classification requests. The CLI command remains listed. The same credential authorizes inline `classification` on supported search/read tools; [the classification guide](OCTOCODE_CLASIFY.md) explains the two routes.

MCP applies these additional restrictions:

- `tools.enabled` / `TOOLS_TO_RUN` is a strict allowlist; `tools.disabled` / `DISABLE_TOOLS` removes names even when allowlisted. Neither bypasses an opt-in above.
- `local.enabled` / `OCTOCODE_ENABLE_LOCAL` controls local tool exposure. CLI commands ignore this catalog switch and both tool lists while retaining their execution policies.
- `tools.enableClone` and `local.beta` are home-only opt-ins. A workspace may set `tools.enableClone: false` or `OCTOCODE_ENABLE_CLONE=false` to disable MCP cloning, but cannot enable it. Storage mode is independent: an enabled clone tool remains visible in `memory` mode and reports the persistent-storage requirement when invoked.

### Enable optional MCP tools

Set home-scope options in `~/.octocode/.octocoderc`:

```jsonc
{
  "local": { "beta": true },
  "tools": { "enableClone": true }
}
```

For classification, obtain a [provider key](AUTHENTICATION.md#classification-key-clasify) and store it without putting it in shell history:

```bash
npx -y octocode config set OCTOCODE_CLASSIFICATION_API --stdin
npx -y octocode config check OCTOCODE_CLASSIFICATION_API
```

Restart MCP after changing configuration. Use the client's `tools/list` to confirm exposure; `npx -y octocode schema` always describes the full CLI catalog. [MCP client environment examples](#mcp-client-env-block) apply the same settings to one client instead.

## Where everything lives

The **Octocode home** is one folder shared by the CLI, the MCP server, the VS Code extension, and installed skills: `~/.octocode` on macOS and Linux, `%USERPROFILE%\.octocode` on Windows. `XDG_CONFIG_HOME` and `APPDATA` are not used. To move it, set `OCTOCODE_HOME=/custom/path` for every product. Use [cache clearing](OCTOCODE_CLI.md#clear-tool-caches) to remove reusable state while preserving checkouts. Current clone outputs live in `clones/`; older versions stored them under `tmp/clone/`. Deleting `credentials.json` and `.key` signs you out.

| Path in the home | Owner | What it holds |
|---|---|---|
| `.octocoderc` | you | Global Octocode settings. See [`.octocoderc`](#octocoderc--octocode-settings). |
| `.env` | you | Trusted environment fallbacks: tokens, the classification key, third-party keys. Only this file may set [home-only keys](#env--environment-fallback). |
| `credentials.json`, `.key` | Octocode | The encrypted GitHub login and its key. `auth logout` removes both. |
| `.credentials.lock` | Octocode | Serializes credential writes between processes. |
| `skills/` | Octocode | Skills installed with `npx octocode skill install`; hosts such as Claude, Cursor, or Codex link to these copies. |
| `stats.json` | Octocode | Clasify calls and tokens. Written only when `storage.stats` (`OCTOCODE_ENABLE_STATS=1`) is on and storage is persistent. |
| `tmp/response/` | cache | GitHub file bodies and directory listings, shared by CLI and MCP to save rate limit. |
| `clones/` | output | Fresh `ghCloneRepo` checkouts, keyed by owner, repo, commit, sparse selection, history depth, and GitHub host. Clean destinations are replaced; local edits are preserved. |
| `tmp/artifact/`, `tmp/outline/`, `tmp/judgment/` | cache | Registry responses, source outlines, and classification answers. |
| `tmp/response-pages/`, `tmp/continuations/`, `tmp/section-assessments/` | retained work | Saved pages, issued continuations, and section assessments. Clearing them invalidates their handles. |
| `tmp/capture/url/` | cache | Captured URL text and images, retained for 24 hours. |
| `tmp/materialize/v3/` | cache | Files `ghStructure` materialized at an immutable commit SHA. A manifest beside each commit directory lists the files; a directory without its manifest is cleared, never reused. |
| `tmp/ratelimit/` | coordination | Last-known GitHub rate-limit state, shared across processes. |
| `tmp/clone-locks/`, `tmp/clone-tmp/`, `tmp/git-home/` | working state | Per-repository clone locks, staging, and an isolated git home. |

One home holds one user's caches and clone outputs. Every clone call checks the caller's GitHub access and fetches a fresh checkout; local tools can read checkouts under `clones/`. On a host that serves several users or tokens, give each tenant its own `OCTOCODE_HOME`.

Clone git runs without your git config. It keeps `PATH`, the proxy variables (`HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`, `NO_PROXY`, either case), and CA bundles (`SSL_CERT_FILE`, `SSL_CERT_DIR`, `GIT_SSL_CAINFO`, `GIT_SSL_CAPATH`, `CURL_CA_BUNDLE`) from the runtime environment; a project `.env` cannot set them.

Native HTTP clients also honor `SSL_CERT_FILE` and `SSL_CERT_DIR` from the process environment. PEM certificates supplement the operating system trust store; unreadable, empty or malformed configured stores fail client construction. Certificate verification stays enabled. `SSL_CERT_DIR` uses the platform path-list separator.

A skill without a project writes reports and state to a named home folder, for example `research/` or `agents-communication/`. Inside a project, skills use `<project>/.octocode/`, which also holds the workspace `.octocoderc` and `.env` and `graph/` (the stored code graph).

### Cache storage and lifecycle

CLI and MCP share the file-based cache roots under `OCTOCODE_HOME`; there is no database or editor-local storage.

`storage.mode: "persistent"` is the default. Set `storage.mode: "memory"` in `<OCTOCODE_HOME>/.octocoderc` (all workspaces) or `<workspace>/.octocode/.octocoderc` (one workspace), or set `OCTOCODE_STORAGE_MODE=memory`, to keep reusable results and continuation state in RAM only. This applies to GitHub responses, registry answers, outlines, classification answers and saved pages. Each runtime uses its own resolved policy and home; one runtime cannot enable another runtime's disk cache.

Memory mode disables clone, directory and exact-file materialization, disk-cache reads and writes, automatic maintenance, rate-limit mirrors and stats writes. File reads still return content without a materialized `localPath`. Explicit graph builds still publish their requested snapshots under the workspace's `.octocode/graph/`, but keep extraction intermediates in memory and never save `.facts` checkpoints. The setting does not delete existing files or credentials; managed language-server installations are separate from tool-result caching.

| Concern | Behavior |
|---|---|
| Cleanup | At most one sweep per 24 hours (`tmp/.last-cache-maintenance`). Aged materialized snapshots are removed under their own locks; old tree staging, rate-limit mirrors and URL captures are swept by age. Entry stores enforce their own TTL and byte budgets. Clone outputs are never expired or evicted. |
| When | Synchronously at native runtime start: once per CLI process and at MCP server start. No background timer. Skipped in `memory` mode or when `tmp/` does not exist. |
| Failure | Best-effort; an unavailable or read-only cache home never blocks the CLI or MCP startup. |
| Ownership | Only Octocode-owned directories; other `tmp/` content stays. |
| Manual | Stop separate MCP processes, then run `npx octocode cache clear` from the workspace. It drains warm CLI servers and clears the configured home's tool caches plus this workspace's graph extraction checkpoints. See [cache clearing](OCTOCODE_CLI.md#clear-tool-caches). |

GitHub responses are cached in memory (4,096 entries, 64 MiB) and, with persistent storage, in `tmp/response/`. File bodies and directory listings (`ghGetFileContent`, the listings `ghStructure` reads) are fresh for 60 seconds, then revalidated by ETag; bodies read at a commit SHA are never revalidated. Search pages (`ghSearchCode`, `ghSearchRepo`, and the `ghSearchHistory` searches) are served from cache for 60 seconds with no request or spacing wait; a page GitHub marks incomplete is never stored. REST history reads (pull-request and commit lists, `ghGetHistoryItem`) send a conditional request every call, so a `304` skips the body. Registry responses and immutable release facts use RAM in both modes and `tmp/artifact/` in persistent mode. Mutable registry answers expire after five minutes. Outlines use runtime-owned RAM and optional `tmp/outline/` entries. Entry and byte limits are per store, not an aggregate process-memory limit. Disk eviction uses entry expiry and oldest disk access; a RAM hit does not update disk recency. Published cache entries stay within each store’s disk byte budget. Writers serialize admission across processes and reserve space before publishing; capacity pressure evicts the oldest entries in batches. The budget counts entry headers and bodies, excluding filesystem allocation overhead, bookkeeping and one temporary atomic-write copy. Failed writes and deletions can temporarily overcount usage; the next pressure or expiry scan reconciles it. Cache writes are not synced to durable storage, so this accounting bound does not guarantee recovery after an OS crash or power loss.

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
- CLI and MCP load both files.
- **Home-only keys** are skipped in a workspace `.env` and listed under `skippedProtected` in `npx octocode config --json`, including when the home `.env` sets the same key. The list is generated from the contract: [home-only environment variables](generated/CONFIG_SETTINGS.md#home-only-environment-variables). GitHub tokens and the classification key are accepted in either file.
- A workspace may **narrow** some home-only switches, never widen them: `OCTOCODE_STORAGE_MODE=memory` (opt the project out of persistence), `OCTOCODE_ENABLE_LOCAL=false`, `OCTOCODE_ENABLE_CLONE=false`, and `OCTOCODE_REDACT_EMAILS=true` apply from a workspace `.env` or `.octocoderc`; the widening value is skipped there (`skippedProtected` for `.env`, a `workspace_config_protected` warning for `.octocoderc`). The local/clone switches affect [MCP exposure](#tool-availability-cli-and-mcp).
- Internal limits (timeouts, retries, waits, page size, caches, concurrency, selection budgets) are not settings: no `.env` or `.octocoderc` can change them. See [fixed internal limits](#fixed-internal-limits).
- Empty values normally allow fallback. A present-but-blank `OCTOCODE_CLASSIFICATION_API` disables classification and blocks file fallback.
- Both files always block these OS and runtime keys; set them in the shell, CI, or the MCP `env` block: `PATH`, `HOME`, `SHELL`, `USER`, `LOGNAME`, `PWD`, `TMPDIR`, `NODE_OPTIONS`, `PYTHON`, `OCTOCODE_HOME`.
- Credential aliases resolve by source first, then by [alias priority](AUTHENTICATION.md#token-environment-variables). Metadata shows names and source files, never values. Keep credential files out of version control.
- `npx octocode config get KEY` prints one value. A secret (a GitHub token, a credential setting's variable, or a name with a credential word such as `KEY`, `TOKEN`, `SECRET`, or `PASSWORD`) is masked as `[REDACTED-KEY]`, followed by where it lives: `(home: ~/.octocode/.env)`, `(workspace: …)`, or `(process environment)`. `--json` reports `value`, `source` (`env`, `home`, or `workspace`), and `path`.

Web-search behavior belongs to each skill. The brainstorming search script queries configured Tavily, Exa and Serper providers and merges their results; it fails with `missingKey` when no provider key is set. Do not assume a keyless fallback.

### `.octocoderc` — Octocode settings

A JSONC file for Octocode's own behavior: `~/.octocode/.octocoderc` (or `$OCTOCODE_HOME/.octocoderc`) and `<project>/.octocode/.octocoderc`, where `<project>` is the process working directory. **Restart the MCP server or start a new agent session after editing.**

```jsonc
// <project>/.octocode/.octocoderc — only what this repo changes
{
  "tools": { "disabled": ["ghSearchRepo"] }
}
```

- Layering is per field: a workspace value replaces only that field. Arrays replace, never concatenate. `null` means "use the next source or the generated default" for every setting, lists included; it never resets a lower layer and never affects another setting.
- Path settings (`local.allowedPaths`, `local.workspaceRoot`, `lsp.configPath`, `lsp.cacheDir`) must be absolute or start with `~`; `~` expands to your OS home once, when settings resolve. A relative path is rejected with a warning.
- Both files rank below every environment source, including the global `.env`. Every setting has an env var; see [generated settings](generated/CONFIG_SETTINGS.md).
- The workspace file loads without a trust flag, like the workspace `.env`. A field bound to a home-only key (for example `local.allowedPaths`, `local.beta`, `github.apiUrl`) is ignored there, with a warning.
- When the working directory is your OS home, `<project>/.octocode` is the Octocode home: its `.octocoderc` and `.env` are read once, as the home layer.
- An unknown key warns with its full path, so a typo like `local.enableLocl` is visible.
- Tool lists restrict [MCP exposure](#tool-availability-cli-and-mcp). A name no tool has (in these lists or `mcp.deferred`) is ignored with an `unknown_tool_name` warning that suggests the nearest tool name.

### How settings override each other

```
Shell env vars / MCP client env block       ← highest
<workspace>/.octocode/.env
~/.octocode/.env
<workspace>/.octocode/.octocoderc
~/.octocode/.octocoderc
Built-in defaults                           ← lowest
```

Precedence is per field: the first source with a valid value wins. Env vars always beat file config. At each tier, workspace beats home, except that a workspace cannot set a home-only key (it may narrow the switches listed above). Missing, blank, `null`, or invalid values normally fall back to the next source; a present-but-blank classification key is the explicit exception described [above](#tool-availability-cli-and-mcp). GitHub and classification credentials use this order; token discovery and OAuth refresh stay in native.

For example, resolve `github.graphqlEnabled` (environment key `OCTOCODE_GITHUB_GRAPHQL`) as follows. Each row lists all sources set in that example:

| Values set | Effective value | Winning source |
|---|---|---|
| Home `.env`: `false`; workspace `.octocoderc`: `true` | `false` | Home `.env`: every environment layer outranks settings files |
| Home `.env`: `false`; workspace `.env`: `true`; workspace `.octocoderc`: `false` | `true` | Workspace `.env` |
| Process env: `false`; workspace `.env`: `true`; home `.env`: `true` | `false` | Process env / MCP client environment |

`config set KEY --stdin` edits the home `.env`, regardless of the current directory; it does not remove a winning workspace or process override. Use `config view` to inspect the effective source or edit an authorized workspace value. Workspace lookup uses the process working directory itself; `WORKSPACE_ROOT` changes tool scope, not where `.octocode/.env` is loaded.

### Invalid settings fall back with warnings

A bad value is skipped and reported once per process on **stderr** (stdout carries CLI results and MCP JSON-RPC), with the file or variable and the reason:

```
octocode: config warning: /repo/.octocode/.octocoderc: lsp.prewarm: Must be one of: targeted, all, off; value ignored [invalid_config]
octocode: config warning: /repo/.octocode/.octocoderc: Failed to parse config file: …; the whole file is ignored [config_load_error]
octocode: config warning: ~/.octocode/.env: OCTOCODE_GITHUB_GRAPHQL is not a valid boolean (true/false/1/0) for github.graphqlEnabled; value ignored [invalid_env_value]
```

| Problem | Effect |
|---|---|
| Unreadable file, invalid JSON, or non-object root | That file is ignored; other layers apply. |
| Invalid field (type, relative path, non-http URL, unknown enum) | That field falls back to the next layer or the default. |
| A section that is not an object (`"network": 5`) | That section is dropped. |
| Unknown key | Reported through the ordinary unknown-key diagnostic; no alias or retired-setting registry applies. |
| Invalid nonblank env value (including `OCTOCODE_CLASSIFICATION_API_HOST`) | Skipped, without printing the value; the next layer applies. |
| Unknown tool name in a tool list | Warning (`unknown_tool_name`), with the nearest tool name. |

A valid configuration can still disable every MCP tool. MCP then fails startup with `No native tools are available`; inspect the [availability controls](#tool-availability-cli-and-mcp). The CLI catalog remains available.

Warnings print on tool calls and `npx octocode config`, not on `npx octocode schema`. `npx octocode config` lists both `.octocoderc` files and their top-level keys; `--json` adds the `diagnostics` array.

## Local configuration view

`npx -y octocode config view` opens a temporary loopback page that edits home or workspace settings, keys, and the Octocode entries in agent configs. `--no-open` prints the session link instead. `--idle-timeout <seconds>` (default 900, 30–3600) closes the server after inactivity. Ctrl+C or **End session** stops it.

**Save to** picks the scope. **Home** writes `~/.octocode/.octocoderc` and `.env` for every project. **Workspace** writes `<project>/.octocode/.octocoderc` and `.env`, where `<project>` is the directory where you ran the command; it overrides home. Each save writes immediately and reloads the page data.

- **Settings:** search by key (`lsp.prewarm`). Booleans are checkboxes, choices are lists, lists are JSON string arrays (`["localSearch","localFetch"]`, or `null` to inherit), text and paths are plain text. **Save** writes the scope; **Use inherited value** removes it from that scope only. **Effective value** and **Effective source** show what applies and from which file. If a home save does not change the effective value, look for a workspace or env override. Home-only settings (`local.allowedPaths`, `local.beta`, `github.apiUrl`, `lsp.configPath`) are disabled in Workspace scope, and `storage.mode` accepts only `memory` there.
- **Keys:** enter a **Key name** (letters, digits, `_`; not starting with a digit) and a one-line **New value**, then **Save key**. The key shows as **Set**; the value is never shown again. **Replace** and **Remove** edit it. Workspace scope refuses home-only keys (`ALLOWED_PATHS`, `WORKSPACE_ROOT`, `GITHUB_API_URL`, `OCTOCODE_BETA`, `OCTOCODE_LSP_CONFIG`, …) but accepts GitHub tokens and `OCTOCODE_CLASSIFICATION_API`. A credential stored in `.octocoderc` (`classification.api`) shows **Stored setting**.
- **Agents:** one card per MCP client file, for example Cursor `~/.cursor/mcp.json` or Claude Code `~/.claude.json`, with scope `home`, `workspace`, or `local`. **Install** (with **Launch with** `npx`, `bunx`, or `pnpm`) adds the entry; **Save agent key** / **Remove key** edit its env; clearing **Enabled** and **Save / update** turns it off; **Remove Octocode** deletes only the Octocode entry. Restart the agent after an edit; client env values can override the shared Octocode files.

Agent discovery checks supported client paths and the current workspace, not other projects or enterprise policies, and reports configuration status, not a live connection. Edits keep other MCP entries. Unsupported or ambiguous layouts, Continue main configuration arrays, symbolic links, and unreadable or malformed files are read only, with configuration warnings; imported Continue blocks report unknown coverage.

| Save error | Fix |
|---|---|
| *Configuration changed* / *Agent config changed* | Another process edited the file; the page reloads it. Review and save again. |
| *Invalid value for …* | Wrong type or not an allowed choice. |
| *may only be configured in home scope* / *cannot be loaded from this .env scope* | Select **Home**, or set the variable in the launching environment. |
| *Values must be a single line* | Remove line breaks. |

Octocode's `.env` and `.octocoderc` are replaced without backup, so a removed secret leaves no copy. An agent client file keeps one rolling hidden backup beside it (`~/.cursor/.mcp.json.bak`); `npx octocode install --rollback <backup>` restores it.

The page uses bundled assets and an authenticated server on `127.0.0.1` at a temporary port. It checks request host and origin and grants no cross-origin access. The printed link holds a one-use session credential; keep it private. A browser extension that can read the page can read the values you enter. Keys in `.env` and credential settings (`classification.api`, `classification.apiHost`) in `.octocoderc` are not encrypted; on Unix, new config files and backups are owner-only. The view does not touch the GitHub login storage. Keep workspace key files out of version control. For terminal entry without shell history, use `npx octocode config set KEY --stdin`.

## MCP client `env` block

Use a client `env` block to set options for that MCP server process. This example enables graph and clone tools; GitHub login and the classification key can come from the shared Octocode home:

```json
{"mcpServers":{"octocode":{"command":"npx","args":["-y","octocode-mcp@latest"],"env":{"OCTOCODE_BETA":"true","OCTOCODE_ENABLE_CLONE":"true","OCTOCODE_LSP_PREWARM":"off"}}}}
```

`npx octocode install --ide cursor` writes a basic server entry; add the optional `env` settings above or use `config view` to edit them. `install --list` shows every client id with its entry status and config path ([CLI `install`](OCTOCODE_CLI.md#install--mcp-client-setup)).

## All settings reference

Defaults, ranges, enums, and env mappings: [generated settings](generated/CONFIG_SETTINGS.md). Notes that the table does not carry:

- **Storage:** `storage.stats` and disk materialization (including `ghCloneRepo`) are disabled by `storage.mode: "memory"`.
- **Language servers:** `~/.octocode/lsp-servers.json` (or the `lsp.configPath` / `OCTOCODE_LSP_CONFIG` file) maps an extension to a server, replacing the built-in one: `{"languageServers":{".ts":{"command":"tsgo","args":["--lsp","-stdio"],"languageId":"typescript"}}}`. Assembly servers come only from this file. `lsp.trustProjectConfig` also trusts a project `.octocode/lsp-servers.json`; `lsp.autoInstall` governs `npx octocode lsp-server install`; `lsp.cacheDir` moves managed installs; a per-server `maxMemoryMb` in `lsp-servers.json` overrides the host-derived memory cap (`0` disables it); `lsp.prewarm` controls background starts. See [LSP server lifecycle](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md).
- **Classification:** [availability and key setup](#tool-availability-cli-and-mcp) live here; [questions, fields and recovery](OCTOCODE_CLASIFY.md) live in the classification guide.

### Fixed internal limits

These are code constants, not settings (one owner: `RuntimeLimits` in `packages/octocode-native/crates/runtime/src/execution/limits.rs`). They have no environment or settings-file overrides.

| Limit | Value |
|---|---|
| Network request timeout | 30 s |
| Retries after a failed request | 3 |
| Longest GitHub rate-limit wait | 45 s |
| Automatic response page | 20,000 characters (`responseChars` per call, up to 50,000) |
| URL captures | 24 h retention |
| Classification requests in flight per endpoint | 10 |
| Context selection per operation | 64 candidates, 131,072 characters, 10 s |
| Language-server memory | 25% of host RAM, between 4096 and 8192 MiB (per-server `maxMemoryMb` overrides) |

Response-cache sizes and the tool-call budget (300 s, one for every surface: CLI, warm server and MCP; an embedder may set its own) are fixed too.

- **GitHub rate limits:** a call waits out a primary or secondary limit, search spacing, or a full code-search window (10 searches a minute) for up to 45 s and then sends; a longer wait fails at once with `rateLimited` and `retryAfter`. Waiting does not spend the request timeout, but the tool call's own deadline still bounds it, and 45 s stays under the 60 s tool-call timeout some MCP hosts apply. Processes sharing a token reserve search slots under one file lock on `tmp/ratelimit/` (keyed by host and a token hash, never the token), so together they stay within one window and keep the spacing. A stamp expires with its window, so a crashed process never wedges it; a stamp dated more than two minutes ahead is ignored. If the file or its directory cannot be read, written or locked, the call goes ahead under this process's own pacing and one warning goes to stderr.
- **Classification gate:** one process-wide gate per provider endpoint allows 10 requests in flight; one tool call may use up to three quarters. Throttling (429/503/529) halves the gate, `Retry-After` pauses every caller, and the gate recovers gradually.
- **Context selection:** units the budget leaves unassessed stay unknown and never spill into the default evidence body; recovery: [context selection](OCTOCODE_TOOLS.md#context-selection).

## GitHub Enterprise

Set `GITHUB_API_URL` (or `github.apiUrl` in the home `.octocoderc`) to the enterprise API root, for example `https://github.mycompany.com/api/v3`. A workspace file cannot set it. Tokens and device login: [AUTHENTICATION.md](AUTHENTICATION.md#github-enterprise).

## Troubleshooting

Start with `npx octocode auth status --json` (token source, identity), `npx octocode config` (paths, set keys), and the MCP client's `tools/list` for MCP availability. `npx octocode schema` lists every CLI tool; `schema <name> --view query` shows its input contract.

| Symptom | Fix |
|---|---|
| Token, login, Enterprise, or `clasify` key problem | [AUTHENTICATION.md troubleshooting](AUTHENTICATION.md#troubleshooting) |
| `ghCloneRepo` missing from MCP | Enable the [clone opt-in](#tool-availability-cli-and-mcp), check MCP tool lists, then restart the server. |
| Clone invocation rejects storage | Use `OCTOCODE_STORAGE_MODE=persistent`; the visibility opt-in does not override storage policy. |
| Local tools missing from MCP | Check `OCTOCODE_ENABLE_LOCAL` / `local.enabled` and the MCP tool lists. |
| Another tool is missing from MCP | Check the [availability matrix](#tool-availability-cli-and-mcp), `TOOLS_TO_RUN` / `tools.enabled`, and `DISABLE_TOOLS` / `tools.disabled`. |
| Slow or timeouts | The 30 s request timeout is fixed; narrow the query with a path, `include` or ranges, or check the network or GitHub Enterprise host. |
| `rateLimited` on GitHub tools | Wait for `retryAfter`, then rerun the call. |
| `clasify` returns `classificationQuotaExhausted` | The provider account has no credit (HTTP 402); add credit and rerun. |
| `clasify` returns `classificationRateLimited` | Wait for the reported retry time; agents sharing a key share its rate limit. |
| A skill's web search is unavailable | Follow that skill's provider instructions; the catalog has no general web-search tool. |
| `stats.json` never written | Set `storage.stats: true` or `OCTOCODE_ENABLE_STATS=1`. |
| `.env` key ignored | A home-only key in a workspace `.env`, or the shell or MCP `env` already sets it; see `npx octocode config --json` → `skippedProtected`. |
| `.env` key not loading | Check the process working directory, then restart. |
| Settings not applied | Restart the MCP server or agent session; check stderr or `config --json` → `diagnostics` confirm the working directory is the workspace you edited. |

## See also

- [Adding config](../skills-dev/octocode-dev/docs/ADDING_CONFIG.md) — contributor guide for settings and credentials
- [Octocode CLI guide](OCTOCODE_CLI.md)
- [Octocode MCP guide](OCTOCODE_MCP.md)
- [Documentation hub](README.md)
