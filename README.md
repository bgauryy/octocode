# Octocode - Agentic Research Platform

<div align="center">
  <img src="https://github.com/bgauryy/octocode/raw/main/packages/octocode-mcp/assets/logo_white.png" width="400px" alt="Octocode Logo">

  [![MCP Community Server](https://img.shields.io/badge/Model_Context_Protocol-Official_Community_Server-blue?style=flat-square)](https://github.com/modelcontextprotocol/servers)
  [![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/bgauryy/octocode)
  [![Website](https://img.shields.io/badge/Website-007ACC?style=for-the-badge&logo=link&logoColor=white)](https://octocode.ai)
  [![YouTube](https://img.shields.io/badge/YouTube-FF0000?style=for-the-badge&logo=youtube&logoColor=white)](https://www.youtube.com/@Octocode-ai)

</div>

**Evidence-first code research for AI agents and developers.**

Octocode gives an agent the full context it needs to change, review, or explain code: real evidence from your **local workspace** and from **external** sources (GitHub repositories, pull requests, and npm packages). One toolset covers all of it: ripgrep and AST structural search, repository tree browsing, precise content fetching, LSP semantic navigation, and binary inspection.

Run it as a **CLI** or an **MCP server**. A **Rust engine** keeps every call fast and token-efficient, minifying and skeletonizing code so an agent reads the shape of a file instead of every byte, from a single file to a mega-repo. It is also the best tool for **cross-repository research and exploration across millions of repositories**.

---

## Table of Contents

- [Why Octocode](#why-octocode)
- [What You Can Do](#what-you-can-do)
- [Tools](#tools)
- [MCP](#mcp)
- [CLI](#cli)
- [Configuration](#configuration)
- [Authentication Methods](#authentication-methods)
- [Security](#security)
- [Language Support](#language-support)
- [Skills](#skills)
- [Architecture](#architecture)
- [Documentation](#documentation)

---

## Why Octocode

Agents write better code from evidence than from assumptions. Octocode turns *guess-driven* work into **research-driven** work. Before an agent changes, reviews, or explains code, it gathers real evidence from your local workspace **and** from GitHub repositories, pull requests, and npm packages, then hands it back as compact, citable context. *Code is truth; context is the map.*

Most tools cover one slice: searching the web, or grepping your repo. Octocode covers the **whole research flow, end to end**:

- **Built for scale.** In organizations with thousands of repositories and endless code, Octocode is the solution: spot a pattern in one repo, follow it through the pull request that introduced it, then trace the same shape across other repos and your own files without leaving the conversation. Clone any repo and study it locally, on any machine.
- **Smart GitHub workflows.** Parallel bulk queries and built-in **next-step hints** keep the agent on the cheapest path: search broadly, read narrowly, trace semantically. Each result points to the natural follow-up.
- **No GitHub required.** Even without GitHub, clone any repository locally and point Octocode's local tools (search, structural AST, LSP, content) at it for the same evidence-first research.
- **Reads the shape, not the noise.** Code is minified and skeletonized on the fly across 70+ languages, so an agent grasps a 100 KB file in a few hundred tokens instead of spending its context on boilerplate.
- **Fast and self-contained.** Search, parsing, semantic navigation, and redaction run in one prebuilt **Rust engine**: quick on a laptop or a mega-repo, with no extra toolchain to install.
- **Safe by default.** Every byte returned to the model is scanned and secrets redacted first (see [Security](#security)).

## What You Can Do

Octocode is useful whenever the next coding step depends on finding and proving context, not guessing it.

| Need | Use Octocode to |
|------|-----------------|
| **Codebase questions** | Search local or GitHub code, read exact regions, browse trees, and carry file/line anchors into the answer. |
| **Implementation research** | Compare patterns across repositories, npm packages, pull requests, commits, and local files before changing code. |
| **Semantic navigation** | Resolve definitions, references, callers/callees, call hierarchy, hovers, symbols, diagnostics, and type relationships through LSP. |
| **Structural matching** | Run AST-shaped searches with patterns or YAML rules so comments and strings do not become false positives. |
| **Large-file context** | Minify, skeletonize, or paginate code so agents spend tokens on relevant structure instead of boilerplate. |
| **Binary or archive inspection** | Inspect archives, compressed streams, native binaries, and strings without leaving the research flow. |
| **Agent workflows** | Expose the same engine through MCP, CLI, OQL, and Agent Skills so assistants and humans use one evidence model. |

### Get Started

Add Octocode to an AI assistant with MCP, or run the same tools directly from
your terminal with the CLI.

**MCP fast install:**

[<img src="https://cursor.com/deeplink/mcp-install-dark.svg" alt="Install in Cursor">](https://cursor.com/en/install-mcp?name=octocode&config=eyJjb21tYW5kIjoibnB4IiwiYXJncyI6WyJvY3RvY29kZS1tY3BAbGF0ZXN0Il19) [<img src="https://img.shields.io/badge/VS_Code-VS_Code?style=flat-square&label=Install%20Server&color=0098FF" alt="Install in VS Code">](https://insiders.vscode.dev/redirect?url=vscode%3Amcp%2Finstall%3F%257B%2522name%2522%253A%2522octocode%2522%252C%2522command%2522%253A%2522npx%2522%252C%2522args%2522%253A%255B%2522octocode-mcp%2540latest%255D%257D) [<img alt="Install in VS Code Insiders" src="https://img.shields.io/badge/VS_Code_Insiders-VS_Code_Insiders?style=flat-square&label=Install%20Server&color=24bfa5">](https://insiders.vscode.dev/redirect?url=vscode-insiders%3Amcp%2Finstall%3F%257B%2522name%2522%253A%2522octocode%2522%252C%2522command%2522%253A%2522npx%2522%252C%2522args%2522%253A%255B%2522octocode-mcp%2540latest%255D%257D)

```bash
# Interactive installer for Cursor, Claude Code, Codex, VS Code, and more
npx octocode install
```

**CLI fast install:**

```bash
npx octocode
```

Authenticate GitHub when you want private repositories or higher API limits:

```bash
npx octocode login
```

### Benchmarks

Latest benchmark output:
[packages/octocode-benchmark/output](https://github.com/bgauryy/octocode/tree/main/packages/octocode-benchmark/output/).

#### ast-grep Structural Comparison

Bars show relative throughput. Higher is better; lower `ms` is better.

What we tested: ast-grep CLI and Octocode structural grep on the same real
repository files, using the same broad AST node-kind searches
(`call_expression`, `call`, `method_invocation`). The goal was to check
structural AST grep compatibility by match count, then measure where time is
spent across Octocode's raw matcher, agent tool path, and public CLI.

This benchmark does not test text grep, LSP navigation, rewriting, or the full
ast-grep rule language. Those are separate capabilities.

```text
Octocode raw native  ████████████████████   17.1 ms median  │  2.0x faster  │  6/6 matched
ast-grep CLI         ██████████░░░░░░░░░░   34.6 ms median  │  baseline     │  6/6 matched
```

`Octocode raw native` means the direct Rust/NAPI `structuralSearchFiles`
matcher: parse and match only, with no tool validation, sanitizer, pagination,
JSON shaping, or Node CLI startup. The agent-facing `localSearchCode` and public
`octocode search --pattern/--rule` paths are intentionally slower because they include those safety
and DX layers.

What was checked: we took ast-grep's benchmark scenario repo list, picked one
deterministic file from each supported repo, asked both tools to find the same
AST node kind in that file, verified identical match counts, then timed the
median run.

Benchmark files:
[runner](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/benchmark/ast-grep/compare-upstream-scenarios.mjs) ·
[scenario manifest](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/benchmark/ast-grep/upstream-outline-scenarios.json) ·
[latest output](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/output/comparison.md) ·
[unified CLI/tool/OQL eval](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/benchmark/octocode/README.md) ·
[agent runbook](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/recipes/agent-benchmark-runbook.md)

---

## Tools

Octocode ships **14 research tools**; the same implementations run identically over [MCP](#mcp) and the [CLI](#cli). `ghCloneRepo` is opt-in for MCP (`ENABLE_CLONE=true`) and enabled by default for CLI; local tools require `ENABLE_LOCAL` (CLI default: on, MCP default: off). All flags: [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CONFIGURATION.md).

**Token knobs.** `concise:true` returns path/title-only lists. `minify` controls file read density: `symbols` = skeleton with line numbers, `standard` = comments/blanks stripped (default), `none` = exact bytes.

### GitHub Tools

| Tool | What it does | Knob |
|------|--------------|------|
| `ghSearchCode` | Code and path search across GitHub by owner, repo, path, filename, extension, and match filters. Accepts 1 to 5 parallel queries. | `concise` |
| `ghGetFileContent` | Read a GitHub file or region: full file, line range, match slice, or paginated chars. | `minify` |
| `ghViewRepoStructure` | Browse a GitHub repository's directory tree before reading files. | |
| `ghSearchRepos` | Discover repositories by keywords, owner, topic, language, stars, forks, size, dates, license, visibility. | `concise` |
| `ghHistoryResearch` | Search PR history, or deep-read one PR: files, patches, comments, reviews, commits. | `concise` |
| `ghCloneRepo` | Clone a repo or sparse subtree into the local cache for local/LSP analysis. **Opt-in** (`ENABLE_CLONE=true`). | `sparsePath` |

### Local Tools

| Tool | What it does | Knob |
|------|--------------|------|
| `localSearchCode` | Local code/text search returning file and line anchors. `mode:"structural"` runs Octocode AST shape queries (`pattern` or `rule`). | `mode` |
| `localViewStructure` | Browse a local directory tree: depth, filters, pagination, metadata. | `concise` |
| `localFindFiles` | Find local files and directories by name, path, regex, extension, size, time, permissions, type. | |
| `localGetFileContent` | Read a local file or region: exact slice, match string, line range, or paginated chars. | `minify` |
| `localBinaryInspect` | Inspect archives, compressed streams, and native binaries: inspect (format/symbols/imports/deps), list, extract, decompress, strings. | |

### Package Search

| Tool | What it does | Knob |
|------|--------------|------|
| `npmSearch` | npm package lookup and keyword search; returns metadata and the source repository for GitHub handoff. | `concise` |

### LSP

| Tool | What it does |
|------|--------------|
| `lspGetSemantics` | Typed semantic navigation. Raw tools support `definition`, `references`, `callers`, `callees`, `callHierarchy`, `hover`, `documentSymbols`, `typeDefinition`, `implementation`, `workspaceSymbol`, `supertypes`, `subtypes`, and `diagnostic`. The CLI uses `octocode search <file> --op <type>` for semantics and `octocode search <file> --symbols` for file or directory symbol outlines. Navigation runs through installed language servers (see the [LSP Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LSP_TOOLS.md)). |

### OQL Search

| Tool | What it does |
|------|--------------|
| `oqlSearch` | Runs typed OQL queries across code, content, structure, files, semantics, repositories, packages, pull requests, commits, artifacts, diff, research, graph, and materialization targets. |

**Per-tool references** (full schemas, fields, and examples) live in **[`docs/mcp`](https://github.com/bgauryy/octocode/tree/main/docs/mcp)**:
- [GitHub Tools](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/GITHUB_TOOLS.md)
- [Local Tools](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LOCAL_TOOLS.md)
- [Binary Tools](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/BINARY_TOOLS.md)
- [LSP Tools](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LSP_TOOLS.md)
- [Tool Behavior Guide](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/TOOL_BEHAVIOR.md)

---

## MCP

The MCP server exposes the Octocode tool catalog directly to your AI assistant over stdio. Install once; the assistant calls tools automatically.

### Install

**Fast install:**

[<img src="https://cursor.com/deeplink/mcp-install-dark.svg" alt="Install in Cursor">](https://cursor.com/en/install-mcp?name=octocode&config=eyJjb21tYW5kIjoibnB4IiwiYXJncyI6WyJvY3RvY29kZS1tY3BAbGF0ZXN0Il19) [<img src="https://img.shields.io/badge/VS_Code-VS_Code?style=flat-square&label=Install%20Server&color=0098FF" alt="Install in VS Code">](https://insiders.vscode.dev/redirect?url=vscode%3Amcp%2Finstall%3F%257B%2522name%2522%253A%2522octocode%2522%252C%2522command%2522%253A%2522npx%2522%252C%2522args%2522%253A%255B%2522octocode-mcp%2540latest%255D%257D) [<img alt="Install in VS Code Insiders" src="https://img.shields.io/badge/VS_Code_Insiders-VS_Code_Insiders?style=flat-square&label=Install%20Server&color=24bfa5">](https://insiders.vscode.dev/redirect?url=vscode-insiders%3Amcp%2Finstall%3F%257B%2522name%2522%253A%2522octocode%2522%252C%2522command%2522%253A%2522npx%2522%252C%2522args%2522%253A%255B%2522octocode-mcp%2540latest%255D%257D)

**Or use the installer (detects your installed clients):**

```bash
# Interactive: detects your installed clients
npx octocode install

# Non-interactive
octocode install --ide cursor
octocode install --ide claude-code
```

https://github.com/user-attachments/assets/de8d14c0-2ead-46ed-895e-09144c9b5071

### Manual Configuration

Add to your MCP client config file:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["octocode-mcp@latest"]
    }
  }
}
```

For GitHub auth, add a token under `env` (see [Authentication Methods](#authentication-methods)).

### Configuration

Set tokens and options as `env` entries here, or machine-wide in `.octocoderc`. See [Configuration](#configuration) for every setting, the home-folder layout, and precedence.

---

## CLI

The CLI exposes the same research engine without an MCP client. Use quick commands for humans, or call raw tools from scripts and CI. It is agent-friendly by design: `npx octocode --help`, `npx octocode context`, and `npx octocode tools <name> --scheme` publish the research protocol, tool descriptions, and exact schemas, while command output returns compact anchors, pagination, and follow-up hints that guide agents through evidence-first research.

### Install

```bash
npx octocode
```

```bash
npx octocode login
npx octocode status
```

### All Commands

Local paths route to local tools; `owner/repo[/path]` targets route to GitHub tools.

| Command | Use it for |
|---------|------------|
| `octocode search <term> <path\|owner/repo>` | Text/regex search, file discovery with `--search path` / `--target files`, AST structural search with `--pattern` / `--rule`, and full OQL with `--query`. Use `--lang` for language/extension scope. |
| `octocode search <path\|owner/repo> --tree` | Browse local or GitHub structure |
| `octocode search <file>` | Read content, line ranges, and matched slices; add `--content-view exact|compact|symbols` or `--raw` |
| `octocode search <file> --symbols` | Show a symbol outline for a file or source tree |
| `octocode search <owner/repo[#N]\|PR-URL> --target pullRequests` | Search or deep-read pull requests |
| `octocode search <owner/repo[/path]> --target commits` | Inspect commit history for a repo, directory, or file |
| `octocode search <keywords...> --target repositories` | Discover GitHub repositories |
| `octocode search <package\|keywords> --target packages` | Search npm and hand off to source repositories |
| `octocode search <file> --op <type> [--symbol <name>] [--line <n>]` | Trace `definition`, `references`, `callers`, `callees`, `callHierarchy`, `hover`, `typeDefinition`, `implementation`, `workspaceSymbol`, `supertypes`, `subtypes`, and `diagnostic`; `documentSymbols` outlines a file directly |
| `octocode search <file> --target artifacts` | Inspect archives, compressed files, and native binaries with `--inspect`, `--list`, `--extract`, `--decompress`, or `--strings` |
| `octocode unzip <archive>` | Unpack an archive to `<octocode-home>/tmp/unzip/<name>-<timestamp>/`, then use local `search --tree`, `search`, and `search <file>` |
| `octocode clone <owner/repo[/path][@branch]>` | Clone a repo or subtree to `<octocode-home>/tmp/clone/` for local/LSP analysis (`ENABLE_CLONE=true`) |
| `octocode cache fetch <owner/repo[@ref]> [path] [--depth file\|tree\|clone]` | Materialize remote content locally and return the absolute `localPath`; reuses the cache or force-refreshes with `--force-refresh` |
| `octocode cache status` | Show size and entry count of clone/tree/binary/unzip cache buckets |
| `octocode cache clear [--clone\|--repos\|--tree\|--binary\|--unzip\|--all]` | Remove cached materialization data |
| `octocode search --query <oql-json>` | Route typed OQL across code, content, structure, files, semantics, repositories, packages, pull requests, commits, artifacts, diff, research, graph, and materialization targets |
| `octocode tools` | List tools, read schemas, or run any MCP tool directly from the terminal |
| `octocode context` | Print agent-facing protocol, system prompt, tool descriptions, and schemas |
| `octocode skill --add <github-folder> --platform <common\|cursor\|claude\|codex\|all>` | Install one GitHub Agent Skill folder into deterministic agent destinations; full flags, no prompt |
| `octocode install` | Configure Octocode in MCP clients |
| `octocode auth` | Manage GitHub authentication with `login`, `logout`, `refresh`, or read-only `status` |
| `octocode login` / `octocode logout` | Open the interactive auth picker or clear stored GitHub credentials |
| `octocode status` | Check token presence, auth identity, MCP installs, sync state, and cache paths |

Full command syntax, flags, examples, and exit codes live in the [CLI Reference](https://github.com/bgauryy/octocode/blob/main/docs/cli/REFERENCE.md).

---

## Configuration

Everything is optional; Octocode runs on sensible defaults. Settings resolve from three sources, in priority order:

```text
environment variables  >  <octocode-home>/.octocoderc  >  built-in defaults
```

1. **MCP / environment variables** (highest): per client or per project, set in your MCP config `env` or your shell.
2. **Global config**: `<octocode-home>/.octocoderc`, machine-wide defaults read by **both the CLI and the MCP server**.
3. **Built-in defaults**: used when neither is set.

**Octocode home** (`<octocode-home>`) holds the global config, encrypted credentials, sessions, stats, and tmp materialization caches. Its location is fixed per platform (there is no override):

| Platform | Location |
|----------|----------|
| macOS | `~/.octocode` |
| Linux | `${XDG_CONFIG_HOME:-~/.config}/.octocode` |
| Windows | `%APPDATA%\.octocode` |

**Set in MCP** (env entries; these win over `.octocoderc`):

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["octocode-mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_xxxxxxxx",
        "ENABLE_LOCAL": "true",
        "ENABLE_CLONE": "false"
      }
    }
  }
}
```

**Set globally** for both the CLI and MCP in `<octocode-home>/.octocoderc` (JSON, comments and trailing commas tolerated; never put tokens here). See the ready-to-copy [example below](#example-octocoderc).

### Common settings

The **Scope** column shows where a setting applies: `Both`, or `MCP` (the CLI ignores it).

| Env var | `.octocoderc` key | Default | Scope | What it does |
|---------|-------------------|---------|-------|--------------|
| `OCTOCODE_TOKEN` / `GH_TOKEN` / `GITHUB_TOKEN` | env only | unset | Both | GitHub token, in priority order. Tokens stay in env, never in `.octocoderc`. |
| `GITHUB_API_URL` | `github.apiUrl` | `https://api.github.com` | Both | API endpoint; use `/api/v3` for GitHub Enterprise. |
| `ENABLE_LOCAL` | `local.enabled` | CLI `true`, MCP `false` | Both | Turns local filesystem + LSP tools on/off. |
| `ENABLE_CLONE` | `local.enableClone` | CLI `true`, MCP `false` | Both | `ghCloneRepo` and directory fetch. Default differs by surface; set `false` to disable in either. |
| `WORKSPACE_ROOT` | `local.workspaceRoot` | `cwd` | Both | Absolute root for resolving relative local paths. |
| `ALLOWED_PATHS` | `local.allowedPaths` | `[]` | Both | Extra path allowlist for local access; empty means home directory only after validation. |
| `TOOLS_TO_RUN` / `ENABLE_TOOLS` / `DISABLE_TOOLS` | `tools.*` | unset | **MCP** | Whitelist, add to, or remove from the registered tool set. The CLI exposes every tool. |
| `REQUEST_TIMEOUT` | `network.timeout` | `30000` | Both | Request timeout in ms (clamped `5000..300000`). |
| `MAX_RETRIES` | `network.maxRetries` | `3` | Both | Retry attempts (clamped `0..10`). |
| `OCTOCODE_OUTPUT_FORMAT` | `output.format` | `yaml` | Both | Response format: `yaml` or `json`. |

> **Local and clone defaults differ by surface.** The **CLI** honors `ENABLE_LOCAL` and defaults local tools on; clone is enabled by default. The **MCP server** honors `ENABLE_LOCAL` and defaults local tools and clone off, so a deployment can control what an assistant may touch. Explicit env or `.octocoderc` values override those defaults.

### Example `.octocoderc`

Drop this at `<octocode-home>/.octocoderc` for machine-wide defaults shared by the CLI and the MCP server. Every field is optional; keep only what you want to change. **Tokens never go here** (use env or `octocode login`).

```jsonc
{
  // GitHub Enterprise users: point at your API endpoint.
  "github": { "apiUrl": "https://api.github.com" },

  "local": {
    "enabled": true,            // overrides the surface default for CLI and MCP
    "enableClone": false,       // false disables ghCloneRepo for CLI and MCP
    "workspaceRoot": "~/code",  // base for relative local paths (absolute or ~)
    "allowedPaths": []          // extra dirs the local tools may read
  },

  "network": { "timeout": 30000, "maxRetries": 3 },

  "output": { "format": "yaml" }  // "yaml" or "json"
}
```

Per-project overrides and custom LSP servers live in a workspace `.octocode/` folder (for example `.octocode/lsp-servers.json`). For every variable, the full `.octocoderc` schema, clone-cache tuning, GitHub Enterprise setup, local-state paths, and precedence details, see the [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CONFIGURATION.md).

---

## Authentication Methods

GitHub-backed tools require authentication. Any one method is enough. Full details: [Authentication Setup](https://github.com/bgauryy/octocode/blob/main/docs/mcp/AUTHENTICATION.md).

### Option 1: Octocode CLI (Recommended)

```bash
npx octocode login
npx octocode status       # verify the active token source
```

Interactive login lets you choose Octocode browser OAuth or `gh auth login`. Octocode OAuth credentials are stored encrypted on disk.

### Option 2: GitHub CLI (also supported)

```bash
gh auth login
```

Octocode reads the `gh` token automatically — no further config needed.

### Option 3: Personal Access Token (also supported)

Set `OCTOCODE_TOKEN`, `GH_TOKEN`, or `GITHUB_TOKEN` in your shell. Required scopes: `repo`, `read:user`, `read:org`.

Create a token at [github.com/settings/tokens](https://github.com/settings/tokens).

> **Security tip**: Never commit tokens to version control. Use environment variables or secure secret management.

---

## Security

**Every byte that reaches the model is scanned and redacted first.** All content (local files, GitHub and npm responses, error messages, and tool outputs) passes through the Rust engine's secret scanner on the way *in* (tool inputs) and on the way *out* (results), so secrets never reach the LLM. The same enforcement runs identically under MCP and the CLI.

- **Secret redaction, in and out.** 300+ provider credential patterns (AWS, Azure, GCP, GitHub, OpenAI, Anthropic, Stripe, Slack, 1Password, and more) plus generic JWTs, PEM/private keys, bearer tokens, database connection strings, and high-entropy strings. Masked values surface a redaction warning so the agent knows.
- **Content sanitized at the source.** Local reads (`localGetFileContent`, ripgrep, structural search, binary, file discovery, structure) and external fetches (GitHub code/files, npm) are scanned as they are read, not only at the boundary.
- **Path safety.** Relative inputs resolve from `WORKSPACE_ROOT` / config / `cwd`, then local reads are bounded to the engine's allowed roots (home by default, plus `ALLOWED_PATHS` and Octocode-registered roots). Symlinks are resolved and the real target is **re-validated**, so a link cannot escape into a blocked location.
- **Sensitive files and directories are blocked by default.** Octocode refuses to read known secret-bearing files and folders wherever they live, returning a redacted error instead of contents. Blocked patterns include:
  - **Keys and certs:** `*.pem`, `*.key`, `*.crt`/`*.cer`/`*.csr`, `*.p12`/`*.pfx`/`*.jks`/`*.keystore`, and SSH keys (`id_rsa`, `*_ed25519`, `authorized_keys`, `known_hosts`, `.ssh/`).
  - **Credentials and tokens:** `.env` / `.env.*`, `.netrc`, `.npmrc`, `.pgpass`, `.git-credentials`, `*_token` / `.token`, `client_secret*.json`, `*service-account*.json`, `auth.json`, `.htpasswd`.
  - **Cloud and infra:** `.aws/`, `.azure/`, `.config/gcloud/`, `.kube/` / `kubeconfig`, `.docker/`, `.terraform/` and `*.tfstate`.
  - **OS and app secret stores:** `.git/`, `secrets/`, `private/`, browser login data (Chrome/Firefox), OS keychains, password managers (`*.kdbx`), shell history files, and crypto wallets.
- **Command safety.** Normal local search runs in-process inside `octocode-engine`. External helpers are fixed per lane, command/argument allowlisted, and run via `spawn` with argument arrays: no shell strings, no injection.
- **Schema validation** runs before any tool executes; untrusted input size and shape are bounded.
- **Credentials.** GitHub auth via env tokens, AES-256-GCM-encrypted on-disk OAuth, or the `gh` CLI; tokens are never logged.

**Full security model, pipeline, and threat coverage: [SECURITY.md](https://github.com/bgauryy/octocode/blob/main/docs/SECURITY.md).** Related: [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/mcp/AUTHENTICATION.md) · [Configuration](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CONFIGURATION.md) · [Credentials](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CREDENTIALS.md)

---

## Language Support

Four code-intelligence axes; three are native to the Rust engine and need no external tooling:

| Axis | What it does | How to use it |
|------|--------------|---------------|
| **Structural AST** | Tree-sitter shape queries (`pattern` or YAML `rule`) across 33 grammars. | `localSearchCode mode:"structural"` · CLI `search --pattern`/`--rule` |
| **Signature outline** | Body-free skeleton with line numbers from real tree-sitter parsing, no heuristics. An anti-growth guard returns the real file when a skeleton wouldn't be smaller. | `minify:"symbols"` · CLI `search <file> --content-view symbols` |
| **Content minification** | Comment/whitespace stripping for 70+ languages and config formats; HTML/Vue/Svelte also minify embedded `<style>`/`<script>`. | `minify:"standard"` (default) |
| **LSP navigation** | definition, references, callers/callees, callHierarchy, hover, typeDefinition, implementation, documentSymbols, via an installed language server; JS/TS also have a native, no-server path. | `lspGetSemantics` · CLI `search --op` / `search --symbols` |

📋 **Full support matrix:** every extension with its exact AST, signature, LSP, and minify capability, machine-generated from the shipped binary, lives in **[`benchmark/SUPPORT.md`](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/benchmark/SUPPORT.md)** (150 extensions: 61 AST, 47 signature, 56 LSP, 89 minify-only). Regenerate or verify with `yarn workspace @octocodeai/octocode-benchmark matrix:check`.

---

## Skills

> [Agent Skills](https://agentskills.io/what-are-skills) are a lightweight, open format for extending AI agent capabilities.
> Browse and install on [**skills.sh/bgauryy/octocode-mcp**](https://www.skills.sh/bgauryy/octocode-mcp) · Skills index: [skills/README.md](https://github.com/bgauryy/octocode/blob/main/skills/README.md)

These are the skills the Octocode team itself uses to build Octocode. **9 skills** live under [`skills/`](https://github.com/bgauryy/octocode/tree/main/skills); the table mirrors the [Skills Index](https://github.com/bgauryy/octocode/blob/main/skills/README.md). ⭐ **[Engineer](https://www.skills.sh/bgauryy/octocode-mcp/octocode-engineer)** is the recommended starting skill.

Install any GitHub skill folder directly from the Octocode CLI:

```bash
# Shared cross-agent location: ~/.agents/skills
npx octocode skill --add bgauryy/octocode-mcp/skills/octocode-engineer --platform common

# Agent-safe install for multiple clients; never prompts
npx octocode skill --add https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-engineer --platform cursor,codex --mode copy --json
```

Platforms: `common` (`~/.agents/skills`), `cursor`, `claude` (Claude Code + Claude Desktop), `codex`, or `all`. Use `--mode symlink` to keep each agent pointed at the Octocode-managed source cache. Full guide: [Skills Guide](https://github.com/bgauryy/octocode/blob/main/docs/SKILLS_GUIDE.md).

| Skill | Directory | Use it when |
|-------|-----------|-------------|
| [**CLI**](https://www.skills.sh/bgauryy/octocode-mcp/octocode) | `octocode/` | You want to research code from the terminal without MCP: local, GitHub, npm, file, repo, PR, or package lookup. |
| ⭐ [**Engineer**](https://www.skills.sh/bgauryy/octocode-mcp/octocode-engineer) | `octocode-engineer/` | You need to understand, implement, review, refactor, or audit code. The default for "work on this code." |
| [**Loop**](https://www.skills.sh/bgauryy/octocode-mcp/octocode-loop) | `octocode-loop/` | The goal and research path are clear and the work needs grounded Act -> Observe -> Learn -> Repeat loops until evidence converges. |
| [**Brainstorming**](https://www.skills.sh/bgauryy/octocode-mcp/octocode-brainstorming) | `octocode-brainstorming/` | The idea is fuzzy: validate prior art, check whether something is worth building, or produce a decision brief. |
| [**RFC Generator**](https://www.skills.sh/bgauryy/octocode-mcp/octocode-rfc-generator) | `octocode-rfc-generator/` | You need a design doc, RFC, architecture proposal, migration plan, or rollout plan before coding. |
| [**Roast**](https://www.skills.sh/bgauryy/octocode-mcp/octocode-roast) | `octocode-roast/` | You want brutal but actionable code critique with severity-ranked findings and fixes. |
| [**Skills**](https://www.skills.sh/bgauryy/octocode-mcp/octocode-skills) | `octocode-skills/` | You are working on Agent Skills themselves: find, evaluate, install, lint, create, or update `SKILL.md` folders. |
| [**Awareness**](https://www.skills.sh/bgauryy/octocode-mcp/octocode-awareness) | `octocode-awareness/` | You need memory, file locks, or verify-before-conclude across runs or concurrent agents in a shared/dirty repo. |
| [**Stats**](https://www.skills.sh/bgauryy/octocode-mcp/octocode-stats) | `octocode-stats/` | You want to visualize Octocode usage: tokens/chars saved, cache hits, errors, and rate limits from `stats.json`. |

---

## Architecture

A yarn-workspaces monorepo. The **MCP server** and the **CLI** are thin front-ends over one shared TypeScript tool core, which delegates every CPU-heavy path to a single **Rust engine** (compiled via [napi-rs](https://napi.rs) to prebuilt `.node` binaries). One tool catalog, one security layer, one response shaper, reached two ways.

```mermaid
graph LR
    CLI["octocode<br/>CLI"]
    MCP["octocode-mcp<br/>MCP server, stdio"]
    VSC["VS Code extension<br/>OAuth + install"]
    CORE["octocode-tools-core<br/>tools, GitHub client, auth, pagination, security bridge"]
    ENGINE["octocode-engine (Rust)<br/>secrets, minify, AST, signatures, ripgrep/diff/YAML, LSP"]
    EXT["GitHub API, local FS + ripgrep, language servers"]

    CLI --> CORE
    MCP --> CORE
    VSC -. starts .-> MCP
    CORE --> ENGINE
    CORE --> EXT
    ENGINE --> EXT

    style ENGINE fill:#1a1a2e,stroke:#e75d2a,color:#fff
```

**Request flow** is identical whether a call arrives over MCP or the CLI:

```text
client → sanitize inputs (Rust) → run tool (GitHub / FS / LSP) → sanitize + YAML-serialize + paginate (Rust) → result + next-step hints
```

**One Rust engine** owns secret detection, sanitization, path/command validation, minification (70+ languages), signature extraction, structural AST search, ripgrep parsing, diff filtering, YAML serialization, and LSP, so the Node event loop stays unblocked and there is no duplicate native loader. It ships prebuilt for darwin (arm64/x64), linux (arm64/x64, gnu + musl), and win32-x64; no Rust toolchain is needed at runtime.

### Packages

| Directory | npm package | Role |
|-----------|-------------|------|
| [`packages/octocode`](https://github.com/bgauryy/octocode/tree/main/packages/octocode) | `octocode` | CLI: quick commands, raw tool runner, skill installs, auth/login/logout, install, status, context. |
| [`packages/octocode-mcp`](https://github.com/bgauryy/octocode/tree/main/packages/octocode-mcp) | `octocode-mcp` | MCP server (stdio) that registers the tool catalog for AI assistants. |
| [`packages/octocode-tools-core`](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core) | `@octocodeai/octocode-tools-core` | Shared tool core: implementations, GitHub client, credentials and token resolution, session, pagination, security bridge. |
| [`packages/octocode-engine`](https://github.com/bgauryy/octocode/tree/main/packages/octocode-engine) | `@octocodeai/octocode-engine` | Rust/napi native engine: security scanning, minification, signatures, structural AST, ripgrep/diff/YAML, LSP. |
| [`packages/octocode-vscode`](https://github.com/bgauryy/octocode/tree/main/packages/octocode-vscode) | `octocode-mcp-vscode` | VS Code extension: GitHub OAuth + multi-editor MCP install. |

---

## Documentation

Website: **[octocode.ai](https://octocode.ai)** · Product docs: **[github.com/bgauryy/octocode/tree/main/docs](https://github.com/bgauryy/octocode/tree/main/docs)** · Index: **[docs/README.md](https://github.com/bgauryy/octocode/blob/main/docs/README.md)**. Product documentation lives in [`docs/`](https://github.com/bgauryy/octocode/tree/main/docs); benchmark methodology, evals, and run artifacts live in [`packages/octocode-benchmark`](https://github.com/bgauryy/octocode/tree/main/packages/octocode-benchmark).

**Docs map**
- [`docs/mcp/`](https://github.com/bgauryy/octocode/tree/main/docs/mcp): MCP server configuration, authentication, tools, workflows, architecture
- [`docs/cli/`](https://github.com/bgauryy/octocode/tree/main/docs/cli): CLI commands, flags, and reference material
- [`docs/`](https://github.com/bgauryy/octocode/tree/main/docs): guides for development, security, and Pi setup
- [`packages/octocode-benchmark/`](https://github.com/bgauryy/octocode/tree/main/packages/octocode-benchmark): benchmark methodology, support matrix, unified eval, recipes, output schema, and run artifacts

**Setup**
- [Authentication Setup](https://github.com/bgauryy/octocode/blob/main/docs/mcp/AUTHENTICATION.md)
- [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CONFIGURATION.md)
- [Using octocode-mcp with Pi](https://github.com/bgauryy/octocode/blob/main/docs/PI/PI_SETUP_GUIDE.md)

**Tool References**
- [GitHub Tools](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/GITHUB_TOOLS.md)
- [Local Tools](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LOCAL_TOOLS.md)
- [Binary Tools](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/BINARY_TOOLS.md)
- [LSP Tools](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LSP_TOOLS.md)
- [Clone & Local Workflow](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CLONE_WORKFLOW.md)
- [Tool Behavior Guide](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/TOOL_BEHAVIOR.md)

**Benchmarks & Evals**
- [Benchmark Summary](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/BENCHMARK.md)
- [Unified CLI/Tool/OQL Eval](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/benchmark/octocode/README.md)
- [Benchmark Runbook](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/recipes/agent-benchmark-runbook.md)
- [Support Matrix](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/benchmark/SUPPORT.md)

**Security, CLI & Skills**
- [Security Model](https://github.com/bgauryy/octocode/blob/main/docs/SECURITY.md)
- [CLI Reference](https://github.com/bgauryy/octocode/blob/main/docs/cli/REFERENCE.md)
- [Skills Guide](https://github.com/bgauryy/octocode/blob/main/docs/SKILLS_GUIDE.md)
- [Skills Index](https://github.com/bgauryy/octocode/blob/main/skills/README.md)

**Shared Internals**
- [Credentials Architecture](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CREDENTIALS.md) · [Session Persistence](https://github.com/bgauryy/octocode/blob/main/docs/mcp/SESSION.md)

### Recommended dev mode: Pi + Octocode

[Pi](https://github.com/earendil-works/pi) is a fast, local-first coding agent whose stated philosophy is *"CLI tools with READMEs (Skills) over MCP."* Pairing it with Octocode gives a lean, evidence-driven dev loop — **Pi edits, Octocode researches**. Two routes, pick by how much surface you need:

- **Skill route — recommended, leanest.** Drop the [`octocode-engineer`](https://www.skills.sh/bgauryy/octocode-mcp/octocode-engineer) skill into Pi's global skills dir. It drives the Octocode **CLI** directly — no MCP transport, minimal token overhead — and Pi auto-discovers it:

  ```bash
  npx octocode skill --add bgauryy/octocode-mcp/skills/octocode-engineer --platform common
  ```

- **Adapter route — full tool surface.** Install [`pi-mcp-adapter`](https://github.com/nicobailon/pi-mcp-adapter) to expose all 14 Octocode MCP tools behind a single ~200-token proxy tool, so servers stay disconnected until a tool is actually called. Enable clone tools with `ENABLE_CLONE=true`.

Tune Pi's behavior with an `APPEND_SYSTEM.md` (a compact starter lives at [`docs/PI/APPEND_SYSTEM.md`](https://github.com/bgauryy/octocode/blob/main/docs/PI/APPEND_SYSTEM.md)). The full walkthrough — adapter install, MCP config scopes, skills, system-prompt tuning, and custom models — is in the [**Pi Setup Guide**](https://github.com/bgauryy/octocode/blob/main/docs/PI/PI_SETUP_GUIDE.md).

### The Manifest

**"Code is Truth, but Context is the Map."** Read the [Manifest of Octocode for Research Driven Development](https://github.com/bgauryy/octocode/blob/main/MANIFEST.md) to understand the philosophy behind Octocode.
