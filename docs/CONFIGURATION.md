# Octocode configuration

Run `octocode config list` from the directory where your agent starts Octocode. It shows each setting’s effective value, default and source. Run `octocode config` in an interactive terminal to open the settings page. With the npm launcher, use `npx -y octocode` in place of `octocode`.

## Where every value comes from

| Location | What belongs there |
|---|---|
| `<launch directory>/.octocode/` | Project settings in `.env` (variables) and `.octocoderc` (JSON). Octocode reads these files from the directory where it starts, even when `WORKSPACE_ROOT` points elsewhere. |
| `<project root>/.octocode/` | Graph snapshots in `graph/` under the graph workspace, unless `local.graphHome` redirects them. A language server can use `lsp-servers.json` under its detected project root when you enable `lsp.trustProjectConfig` in trusted settings. |
| `~/.octocode/` (user home) | Shared `.env` and `.octocoderc`, saved GitHub logins (`credentials.json` and `.key`), language server definitions (`lsp-servers.json`) and installs (`lsp/`), installed `skills/`, checkouts (`clones/`) and their shared Git objects (`clone-store/`), cached answers (`cache/`) and resumable results (`tmp/`). |

**Settings precedence, highest first.** Process environment (including MCP’s `env`) → project `.env` → user `.env` → project `.octocoderc` → user `.octocoderc` → default. Each setting defines what a project may change. For example, a project can disable cloning. Enabling it requires user or process configuration. [Permissions by setting](#all-settings-and-defaults)

**API keys, highest first.** Process environment → user `.env` → project `.env`. A key in the user `.env` (GitHub tokens, Tavily, Exa, Serper, DeepSeek, ScrapingAnt) beats the same key in a project `.env`, so a repository only fills a key you have not set and cannot swap in its own; the classification key is user-only. The CLI, the MCP server and skill scripts (`scripts/keys.mjs list` in a skill that calls a provider) read keys in this order.


Run `octocode config home` to locate your user settings. The Windows default is `%USERPROFILE%\.octocode`. Set `OCTOCODE_HOME` to use another directory directly. When the project and user settings are the same files, Octocode reads them once as user settings. [Storage and cleanup](STORAGE_AND_LIMITS.md)

## Set a value

| Scope | Example |
|---|---|
| One command (POSIX shell) | `OCTOCODE_STORAGE_MODE=memory octocode config get storage.mode` |
| One MCP server | Put `"env": {"OCTOCODE_BETA":"true"}` in its server entry. Restart MCP after changing its configuration. |
| All your projects | `octocode config set local.beta true` writes to the user `.env`. Undo with `octocode config unset local.beta`. |
| One project | Put `{"tools":{"disabled":["ghSearchHistory"]}}` in the workspace `.octocoderc`. Use `KEY=VALUE` in `.env`. |

For a secret, use `octocode config set OCTOCODE_TYPESAFE_KEY --stdin`. Verify it with `octocode config check OCTOCODE_TYPESAFE_KEY` (exit 0 if set, 1 if unset). Keep credential files out of version control.

For a custom language server, put its command in your user `lsp-servers.json`. This entry uses an installed `tsgo` for TypeScript files.

```json
{"languageServers":{".ts":{"command":"tsgo","args":["--lsp","-stdio"],"languageId":"typescript"}}}
```

### Change the classification host

Use `octocode config set classification.apiHost https://gateway.example.com` (or set `OCTOCODE_TYPESAFE_HOST`) for a compatible gateway; unset, calls go to `https://api.typesafe.ai`. It receives your key and selected evidence. Use an HTTPS root URL without a path. HTTP is also allowed on loopback. [Classification settings and billing](OCTOCODE_CLASIFY.md)

## Tool availability CLI and MCP

The CLI has 14 tools by default. MCP starts with 7: the external section.

The tools fall into four sections, each with one switch for MCP:

| Section | MCP tools | Availability |
|---|---|---|
| External (GitHub and packages) | `ghSearchRepo`, `ghSearchCode`, `ghStructure`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem`, `artifactSearch` | Always on. |
| Local | `structureSearch`, `localSearch`, `localFetch`, `astSearch`, `lspSearch` | Off by default. Set `local.enabled=true` (`OCTOCODE_ENABLE_LOCAL=true`). [Turn local tools on](#local-profile) |
| Local | `graphBuild`, `graphQuery` | Install the separate `@octocodeai/octocode-graph` MCP server with local tools enabled. Its launcher opts into the graph tools. |
| Local | `ghCloneRepo` | Set `tools.enableClone=true` with the local tools on: they are the only readers of a checkout. Clones require persistent storage on either interface. |
| Security | `securityCheck` | Off by default. Set `tools.enableSecurity=true` (`OCTOCODE_ENABLE_SECURITY=true`). [What advisory checks send](SECURITY.md#advisory-egress) |
| Clasify | `clasify` | On whenever a nonblank `OCTOCODE_TYPESAFE_KEY` is set, on both interfaces. |

MCP applies `tools.enabled` as an allowlist and gives `tools.disabled` priority. Allowlisted tools still need their opt-ins. The CLI includes clone and advisory commands by default and ignores both MCP lists; the graph commands `graphBuild` and `graphQuery` run through the separate graph CLI (`npx -y @octocodeai/octocode-graph graphBuild '<json>'`). [Tool examples](OCTOCODE_TOOLS.md)

Each tool's section is declared in the tool contract, and `octocode schema` prints it per tool. [Design and reasoning](rfcs/tool-sections.md)

### Local profile

An MCP session starts with the GitHub and package tools only. To search and read code on this machine, turn the local section on in the MCP server entry:

```json
"env": {"OCTOCODE_ENABLE_LOCAL": "true"}
```

Or run `octocode config set local.enabled true` once: it applies to every MCP server on this machine. `octocode mcp install --enable-local true` writes the variable into a client's config. A repository's `.octocoderc` can turn local tools off but never on. Restart MCP afterwards.

| | Default | With local tools |
|---|---:|---:|
| MCP tools listed | 7 | 12 |
| Tool definitions sent to the model | 21.6k characters | 41.4k characters |

Measured on the 26 November 2026 build as the JSON of each `tools/list` entry. The server instructions gain the route lines that name the local tools. Add `OCTOCODE_ENABLE_CLONE=true` to clone a repository and search it with them.

The difference matters where the host loads every tool definition into the prompt. A host that loads definitions on demand, as Claude Code does by default, already leaves them out. The CLI is not affected: it runs every local tool with no setting.

## GitHub token resolution

CLI and MCP check these sources in order for the configured host.

Process token → workspace `.env` token → user `.env` token → saved Octocode login → OS credential store → GitHub CLI login → anonymous access where supported. Within one environment source the order is `GH_TOKEN`, then `GITHUB_TOKEN`. The source decides first, so a process `GITHUB_TOKEN` wins over a project `GH_TOKEN`.

Run `octocode auth status --json` to see the selected host, account and `tokenSource`. [Token sources, Enterprise and recovery](AUTHENTICATION.md#github-token-resolution)

## Proxies and certificates

Set proxy and TLS variables in the shell, CI or MCP process environment. `SSL_CERT_FILE` and `SSL_CERT_DIR` add trusted PEM certificates. Clone Git also accepts `GIT_SSL_CAINFO`, `GIT_SSL_CAPATH` and `CURL_CA_BUNDLE`.

## Troubleshooting

- **Unexpected value or ignored file.** Run `octocode config get local.beta --json` with the setting you want to inspect. `octocode config --json` lists loaded files and diagnostics, including protected and already-set variables.
- **Missing tool.** Check the MCP server’s working directory, opt-ins and tool lists. Compare with the [availability table](#tool-availability-cli-and-mcp).

<a id="all-settings-reference"></a>

## All settings and defaults

<details>
<summary>Show setting names, environment variables, defaults and workspace permissions</summary>

<!-- settings-table:start (generated from the config contract; do not edit) -->

| `.octocoderc` setting | Environment variable | Type | Default | Sensitive | Workspace may set | What it does |
|---|---|---|---|---|---|---|
| `github.apiUrl` | `GITHUB_API_URL` | url | `https://api.github.com` | no | no | GitHub REST API root. GitHub Enterprise commonly uses /api/v3. |
| `local.enabled` | `OCTOCODE_ENABLE_LOCAL` | boolean | `false` | no | only `false` | Expose the local section through MCP: the local filesystem tools, ghCloneRepo (with tools.enableClone) and the graph tools. Off, MCP lists the GitHub and package tools only. CLI tools remain available; path and content security apply on both surfaces. Restart MCP after changing this setting. Home-only, so a repository can turn it off for MCP but cannot turn it on. |
| `local.beta` | `OCTOCODE_BETA` | boolean | `false` | no | no | Enable beta tools on their owning MCP server. The octocode-graph launcher opts into its graph tools by default. Restart the server after changing its environment. |
| `local.allowedPaths` | `ALLOWED_PATHS` | list | `[]` | no | no | Extra absolute or home-relative roots added to the allowed roots (workspace root and OCTOCODE_HOME). The environment value is a comma-separated list. |
| `local.workspaceRoot` | `WORKSPACE_ROOT` | path | `the working directory` | no | no | Optional absolute or home-relative workspace root. |
| `local.graphHome` | `OCTOCODE_GRAPH_HOME` | path | `<workspace>/.octocode/graph` | no | no | Directory in which graphBuild keeps its snapshots, in place of the workspace's .octocode/graph, shared by every workspace: a build then writes nothing in the repository and may scan any allowed directory, such as a ghCloneRepo checkout. Octocode creates the directory if needed and writes only in its own octocode-graph folder there, never reading or removing anything else; the setting grants local tools no read access to it. A build of a path that contains it is refused. Home-only, so a repository cannot choose where Octocode writes. |
| `tools.enabled` | `TOOLS_TO_RUN` | list | `unset (every tool)` | no | yes | Strict MCP tool allowlist; other MCP prerequisites still apply. The CLI ignores this list. |
| `tools.disabled` | `DISABLE_TOOLS` | list | `unset (none)` | no | yes | Tools removed from MCP discovery and admission. The CLI ignores this list. |
| `tools.enableClone` | `OCTOCODE_ENABLE_CLONE` | boolean | `false` | no | only `false` | Expose ghCloneRepo through MCP, with the local tools that read its checkout (local.enabled). CLI remains available. Clone execution requires persistent storage on both surfaces. Restart MCP after changing this setting. |
| `tools.enableSecurity` | `OCTOCODE_ENABLE_SECURITY` | boolean | `false` | no | only `false` | Expose securityCheck through MCP: it asks OSV (api.osv.dev, operated by Google) about packages, sending each public package name and version it checks. Off, MCP leaves it out and sends nothing. The CLI always offers it, and a CLI call sends the package name to OSV only when you run it. Restart MCP after changing this setting. Home-only, so a repository can turn it off for MCP but cannot turn it on. |
| `network.allowPrivateRegistry` | `OCTOCODE_ALLOW_PRIVATE_REGISTRY` | boolean | `false` | no | no | Allow private, loopback, or link-local artifact registries. Security-sensitive SSRF escape hatch; opt in only for trusted enterprise registries. |
| `lsp.autoInstall` | `OCTOCODE_LSP_AUTO_INSTALL` | prompt \| off \| auto | `prompt` | no | no | Whether `octocode lsp-server install` may download a managed language server: prompt (ask), off, or auto. |
| `lsp.trustProjectConfig` | `OCTOCODE_TRUST_PROJECT_LSP_CONFIG` | boolean | `false` | no | no | Trust a project .octocode/lsp-servers.json. Security-sensitive: a trusted project config can launch server binaries. |
| `lsp.trustFetchedCode` | `OCTOCODE_LSP_TRUST_FETCHED_CODE` | boolean | `false` | no | only `false` | Let lspSearch start language servers on code Octocode fetched: ghCloneRepo and ghStructure materialize checkouts and URL captures under its home. Off by default, because a language server can run code from the repository it opens; text, AST and graph tools read that code without running anything. Security-sensitive: turning it on lets a repository you fetched run code on this machine when lspSearch opens it. Home-only, so a repository cannot turn it on. |
| `lsp.prewarm` | `OCTOCODE_LSP_PREWARM` | targeted \| all \| off | `targeted` | no | yes | Language-server prewarm: targeted warms the server of the file a localSearch, localFetch or astSearch row names and of an lspSearch call the response offers; all also warms files named by other local tools and, at MCP initialize, the workspace's top three languages; off disables prewarm. Only languages with a configured server start, and each start indexes a whole project: targeted avoids unrelated server starts, but a lookup before any read or search can take seconds. |
| `output.redactEmails` | `OCTOCODE_REDACT_EMAILS` | boolean | `false` | no | only `true` | Mask email addresses such as GitHub commit authors. |
| `output.redactUuid` | `OCTOCODE_REDACT_UUID` | boolean | `false` | no | only `true` | Mask UUIDs (8-4-4-4-12 hexadecimal) in every tool result. Off by default: a UUID is an identifier, not a secret. |
| `output.mcpResult` | `OCTOCODE_MCP_RESULT` | json \| text | `json` | no | no | Format of an MCP tools/call reply: json sends the compact JSON result as one text block; text sends the text view the CLI prints (content lines, then `# key: value` lines, each continuation as a runnable `{"tool","input"}` call), which a client must not parse as JSON. |
| `security.credentialFiles` | `OCTOCODE_CREDENTIAL_FILES` | withhold \| allow | `withhold` | no | only `withhold` | Whether local read and search tools may read the known credential files (.env and .env.*, credentials.json, .netrc, .npmrc, id_rsa*, *.pem and the like): withhold them, or allow them in the workspace and allowed paths so results match ripgrep. Known secret formats stay redacted. Credential directories such as .ssh, .aws and .git, the Octocode home, and known credential files directly in your home or in its dot directories (~/.npmrc, ~/.codex/auth.json) stay withheld; every other file, such as ~/.zshrc or a shell history, follows the allowed roots like any file. Graph snapshots, clasify and classification never read these files, and allow keeps saved local results in memory, so a cursor ends with its process. Security-sensitive: redaction recognizes known secret formats only, so a secret in no known format can appear in results. Home-only, so a repository cannot turn it on. |
| `storage.mode` | `OCTOCODE_STORAGE_MODE` | persistent \| memory | `persistent` | no | only `memory` | Persistent saves continuation state and provider answers to disk; memory retains them only in the current runtime. Fresh local requests reuse only in-memory results keyed by the content they read; GitHub and registry answers repeat as their origin allows. Explicit graph snapshots are outputs; interrupted extraction checkpoints follow this setting. |
| `storage.cloneCacheLimit` | `OCTOCODE_CLONE_CACHE_LIMIT` | string | `5GiB` | no | no | Byte cap for the clean ghCloneRepo checkouts under the Octocode home, as a whole number with an optional binary unit (B, KiB, MiB, GiB, TiB; K, M, G, T, KB, MB, GB and TB mean the same): "5GiB", "512MiB", "1073741824". After a new checkout is published, the least recently used checkouts are removed until the total fits. A checkout in use, one with local changes, and the one just published stay. 0 means no cap. An invalid value is reported and the default applies. The shared object stores behind the checkouts are not counted. Home-only, so a repository cannot make Octocode discard your checkouts. |
| `classification.api` | `OCTOCODE_TYPESAFE_KEY` | key | `unset (classification off)`; [get a key](https://docs.typesafe.ai/introduction) | yes | no | TypeSafe classification API key. A nonblank value exposes clasify and read-tool classifiers in MCP and the CLI; without one, a CLI clasify call answers where to get a key. Discovery makes no provider request. Credential values are omitted from resolved configuration and diagnostics. Home-only, so a repository cannot turn classification on or send your evidence under its own provider account. |
| `classification.apiHost` | `OCTOCODE_TYPESAFE_HOST` | url | `https://api.typesafe.ai` | no | no | TypeSafe classification API root, for a proxy, gateway or compatible provider that serves the same API. Calls require HTTPS except on loopback, and an API root with no path. Home-only, so a repository cannot redirect your key. |
| `classification.model` | `OCTOCODE_CLASSIFICATION_MODEL` | string | `jev-latest` | no | yes | Classification model for read-tool classifiers and clasify; a clasify row's model overrides it. Pin an exact model (for example jev-1.13.0) for reproducible results; an alias can change between fresh evaluations. |
| — | `TAVILY_API_KEY` | key | `unset`; [get a key](https://app.tavily.com/) | yes | yes | Tavily search API credential used by the octocode-brainstorming skill. |
| — | `EXA_API_KEY` | key | `unset`; [get a key](https://dashboard.exa.ai/) | yes | yes | Exa search API credential used by the octocode-brainstorming skill. |
| — | `SERPER_API_KEY` | key | `unset`; [get a key](https://serper.dev/) | yes | yes | Serper search API credential used by the octocode-brainstorming skill. |
| — | `DEEPSEEK_API_KEY` | key | `unset`; [get a key](https://platform.deepseek.com/api_keys) | yes | yes | DeepSeek search API credential used by the octocode-brainstorming skill. |
| — | `TAVILY_BASE_URL` | url | `https://api.tavily.com` | no | no | Tavily API root; receives TAVILY_API_KEY. Home-only, so a repository cannot redirect your key. |
| — | `EXA_BASE_URL` | url | `https://api.exa.ai` | no | no | Exa API root; receives EXA_API_KEY. Home-only, so a repository cannot redirect your key. |
| — | `SERPER_BASE_URL` | url | `https://google.serper.dev` | no | no | Serper API root; receives SERPER_API_KEY. Home-only, so a repository cannot redirect your key. |
| — | `DEEPSEEK_BASE_URL` | url | `https://api.deepseek.com` | no | no | DeepSeek API root; receives DEEPSEEK_API_KEY. Home-only, so a repository cannot redirect your key. |
| — | `GH_TOKEN` | key | `unset (octocode auth login also works)` | yes | yes | GitHub CLI token |
| — | `GITHUB_TOKEN` | key | `unset (octocode auth login also works)` | yes | yes | GitHub Actions token |
| — | `SCRAPING_ANT` | key | `unset`; [get a key](https://app.scrapingant.com/) | yes | yes | ScrapingAnt API credential used by the octocode-scraping skill. |
| — | `HTTPS_PROXY` | url | `unset` | yes | no | Proxy for HTTPS requests and clone git. Shell, CI or MCP env block only. |
| — | `HTTP_PROXY` | url | `unset` | yes | no | Proxy for HTTP requests and clone git. Shell, CI or MCP env block only. |
| — | `ALL_PROXY` | url | `unset` | yes | no | Proxy for every protocol. Shell, CI or MCP env block only. |
| — | `NO_PROXY` | string | `unset` | no | no | Hosts that skip the proxy. Shell, CI or MCP env block only. |
| — | `SSL_CERT_FILE` | path | `the OS trust store` | no | no | Extra PEM certificates for native HTTP clients and clone git. Shell, CI or MCP env block only. |
| — | `SSL_CERT_DIR` | path | `the OS trust store` | no | no | Directories of extra PEM certificates. Shell, CI or MCP env block only. |
| — | `OCTOCODE_GITHUB_CLIENT_ID` | string | `the built-in GitHub app` | no | no | GitHub OAuth client override, for GitHub Enterprise. |
| — | `OCTOCODE_HOME` | path | `~/.octocode` | no | no | Overrides the Octocode configuration directory (default: <os-home>/.octocode) |

<!-- settings-table:end -->

</details>
