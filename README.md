# Octocode - Agentic Research Platform

<div align="center">
  <img src="https://github.com/bgauryy/octocode/raw/main/packages/octocode-mcp/assets/logo_white.png" width="400px" alt="Octocode Logo">

  [![MCP Community Server](https://img.shields.io/badge/Model_Context_Protocol-Official_Community_Server-blue?style=flat-square)](https://github.com/modelcontextprotocol/servers)
  [![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/bgauryy/octocode)

  [![Website](https://img.shields.io/badge/Website-007ACC?style=for-the-badge&logo=link&logoColor=white)](https://octocode.ai)
  [![YouTube](https://img.shields.io/badge/YouTube-FF0000?style=for-the-badge&logo=youtube&logoColor=white)](https://www.youtube.com/@Octocode-ai)

</div>

**Evidence-first code research for AI agents and developers.**

Evidence from your **local workspace** and **external** sources (GitHub repos, PRs, npm). One toolset: ripgrep + AST search, trees, precise reads, and LSP — as a **CLI** or **MCP server**, backed by a **Rust engine** for fast, token-efficient results across single files or mega-repos.

---

## Table of Contents

- [Quick Start](#quick-start)
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
- [Troubleshooting](#troubleshooting)

---

## Quick Start

**1. Run the Octocode CLI with `npx`**

```bash
npx octocode --help
```

**2. Authenticate with GitHub** — optional, but unlocks private repositories and higher API rate limits:

```bash
npx octocode auth login
npx octocode status       # verify the active token source
```

**3. Choose your interface.** The same engine and 12 tools run identically either way.

**🖥️ CLI** — research straight from your terminal:

```bash
npx octocode
```

**🤖 MCP** — one-click install:

- [<img src="https://cursor.com/deeplink/mcp-install-dark.svg" alt="Install in Cursor">](https://cursor.com/en/install-mcp?name=octocode&config=eyJjb21tYW5kIjoibnB4IiwidHlwZSI6InN0ZGlvIiwiYXJncyI6WyJAb2N0b2NvZGVhaS9tY3BAbGF0ZXN0Il19)
- [<img src="https://img.shields.io/badge/VS_Code-Install_Server-0098FF?style=flat-square&logo=visualstudiocode&logoColor=white" alt="Install in VS Code">](https://insiders.vscode.dev/redirect/mcp/install?name=octocode&config=%7B%22command%22%3A%22npx%22%2C%22type%22%3A%22stdio%22%2C%22args%22%3A%5B%22%40octocodeai%2Fmcp%40latest%22%5D%7D)
- [<img src="https://img.shields.io/badge/VS_Code_Insiders-Install_Server-24bfa5?style=flat-square&logo=visualstudiocode&logoColor=white" alt="Install in VS Code Insiders">](https://insiders.vscode.dev/redirect/mcp/install?name=octocode&config=%7B%22command%22%3A%22npx%22%2C%22type%22%3A%22stdio%22%2C%22args%22%3A%5B%22%40octocodeai%2Fmcp%40latest%22%5D%7D&quality=insiders)
- [<img src="https://img.shields.io/badge/Windsurf-Install_Server-1a1a1a?style=flat-square&logoColor=white" alt="Install in Windsurf">](windsurf://mcp/install?name=octocode&config=%7B%22command%22%3A%22npx%22%2C%22type%22%3A%22stdio%22%2C%22args%22%3A%5B%22%40octocodeai%2Fmcp%40latest%22%5D%7D)
- [<img src="https://kiro.dev/images/add-to-kiro.svg" alt="Install in Kiro">](https://kiro.dev/launch/mcp/add?name=octocode&config=%7B%22command%22%3A%22npx%22%2C%22type%22%3A%22stdio%22%2C%22args%22%3A%5B%22%40octocodeai%2Fmcp%40latest%22%5D%7D)
- [<img src="https://goose-docs.ai/img/extension-install-dark.svg" alt="Install in Goose">](https://goose-docs.ai/extension?cmd=npx&arg=%40octocodeai%2Fmcp%40latest&id=octocode&name=octocode&description=Evidence-first%20code%20research%20for%20AI%20agents)
- [<img src="https://files.lmstudio.ai/deeplink/mcp-install-light.svg" alt="Install in LM Studio">](https://lmstudio.ai/install-mcp?name=octocode&config=eyJjb21tYW5kIjoibnB4IiwidHlwZSI6InN0ZGlvIiwiYXJncyI6WyJAb2N0b2NvZGVhaS9tY3BAbGF0ZXN0Il19)

**Claude Code:**

```bash
claude mcp add-json octocode --scope user '{"command":"npx","type":"stdio","args":["@octocodeai/mcp@latest"]}'
```

**Any other client:** `npx octocode install`

➡️ Learn more at **[octocode.ai](https://octocode.ai)**.

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
| **Agent workflows** | Same engine via MCP, CLI, OQL, and Agent Skills. |

See [Quick Start](#quick-start) to install in your terminal or AI assistant.

---

## Tools

**12 always-on tools** (same on [MCP](#mcp) and [CLI](#cli)). Local tools on by default (`ENABLE_LOCAL=false` to disable). `ghCloneRepo` is MCP opt-in (`ENABLE_CLONE=true`), CLI on by default. Flags: [Configuration](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md).

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

### Package Search

| Tool | What it does | Knob |
|------|--------------|------|
| `npmSearch` | npm package lookup and keyword search; returns metadata and the source repository for GitHub handoff. | `concise` |

### LSP

| Tool | What it does |
|------|--------------|
| `lspGetSemantics` | Typed semantic navigation. Raw tools support `definition`, `references`, `callers`, `callees`, `callHierarchy`, `hover`, `documentSymbols`, `typeDefinition`, `implementation`, `workspaceSymbol`, `supertypes`, `subtypes`, and `diagnostic`. The CLI uses `npx octocode search <file> --op <type>` for semantics and `npx octocode search <file> --symbols` for file or directory symbol outlines. Navigation runs through installed language servers (see the [LSP Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#lsp-tools-reference)). |

### OQL Search

| Tool | What it does |
|------|--------------|
| `oqlSearch` | Runs typed OQL queries across code, content, structure, files, semantics, repositories, packages, pull requests, commits, diff, research, graph, and materialization targets. |

Full schemas, fields, and examples for every tool live in [`docs/OCTOCODE_TOOLS.md`](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md) (linked under [Documentation](#documentation)).

---

## MCP

The MCP server exposes the Octocode tool catalog directly to your AI assistant over stdio.

https://github.com/user-attachments/assets/de8d14c0-2ead-46ed-895e-09144c9b5071

### Manual Configuration

Add to your MCP client config. Pick the package that matches the version you want:

**New Octocode (Rust-powered engine)** — use `@octocodeai/mcp`:

```json
{
  "octocode": {
    "command": "npx",
    "type": "stdio",
    "args": [
      "@octocodeai/mcp@latest"
    ]
  }
}
```

**Classic octocode-mcp** — use `octocode-mcp`:

```json
{
  "octocode": {
    "command": "npx",
    "type": "stdio",
    "args": [
      "octocode-mcp@latest"
    ]
  }
}
```

Add a GitHub token and options under `env` — see [Authentication](#authentication-methods) and [Configuration](#configuration).

---

## CLI

Same research engine, no MCP client needed. Local paths route to local tools; `owner/repo[/path]` routes to GitHub.

```bash
npx octocode auth login   # authenticate once
npx octocode status       # verify setup
npx octocode --help       # full usage
```

### Commands

#### Search

| Command | What it does |
|---------|--------------|
| `npx octocode search <term> <path\|owner/repo>` | Text, regex, AST structural (`--pattern`), or full OQL (`--query`) |
| `npx octocode search <path\|owner/repo> --tree` | Browse directory or repository structure |
| `npx octocode search <file>` | Read file content; `--content-view exact\|compact\|symbols` or `--raw` |
| `npx octocode search <file> --symbols` | Symbol outline for a file or source tree |
| `npx octocode search --query <oql-json>` | Full OQL across all target types (code, commits, PRs, packages, …) |

#### More commands

- **GitHub & npm** — `npx octocode search <…> --target repositories|packages|pullRequests|commits|diff`
- **LSP** — `npx octocode search <file> --op definition|references|callers|callees|hover|diagnostic|callHierarchy` (`--symbol`, `--line` to narrow)
- **Cache & clone** — `npx octocode clone`, `npx octocode cache fetch|status|clear`
- **Skills** — `npx octocode skill --list | --name <skill> | --add <github-path> | --install-all`
- **Language servers** — `npx octocode lsp-server list|install|status|uninstall|clean`
- **Setup & introspection** — `npx octocode install`, `npx octocode auth`, `npx octocode status`, `npx octocode tools`, `npx octocode context`

Full syntax, flags, and exit codes: [Octocode CLI Guide](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md)

---

## Configuration

Everything is optional; Octocode runs on sensible defaults. Settings resolve from three sources, in priority order:

```text
environment variables  >  <octocode-home>/.octocoderc  >  built-in defaults
```

1. **MCP / environment variables** (highest): per client or per project, set in your MCP config `env` or your shell.
2. **Global config**: `<octocode-home>/.octocoderc`, machine-wide defaults read by **both the CLI and the MCP server**.
3. **Built-in defaults**: used when neither is set.

**Octocode home** (`<octocode-home>`) holds the global config, encrypted credentials, sessions, stats, and tmp materialization caches. It defaults by platform and can be overridden with `OCTOCODE_HOME`:

| Platform | Location |
|----------|----------|
| macOS | `~/.octocode` |
| Linux | `${XDG_CONFIG_HOME:-~/.config}/.octocode` |
| Windows | `%APPDATA%\.octocode` |

Set values as MCP `env` entries (per client; these win over `.octocoderc`) or globally in `<octocode-home>/.octocoderc` (JSON with comments). **Tokens never go in `.octocoderc`** — use `env` or `npx octocode auth login`.

### Common settings

The **Scope** column shows where a setting applies: `Both`, or `MCP` (the CLI ignores it).

| Env var | `.octocoderc` key | Default | Scope | What it does |
|---------|-------------------|---------|-------|--------------|
| `OCTOCODE_HOME` | env only | platform default | Both | Overrides the Octocode data directory for config, credentials, sessions, stats, and caches. |
| `OCTOCODE_TOKEN` / `GH_TOKEN` / `GITHUB_TOKEN` | env only | unset | Both | GitHub token, in priority order. Tokens stay in env, never in `.octocoderc`. |
| `GITHUB_API_URL` | `github.apiUrl` | `https://api.github.com` | Both | API endpoint; use `/api/v3` for GitHub Enterprise. |
| `ENABLE_LOCAL` | `local.enabled` | `true` | Both | Turns local filesystem + LSP tools on/off; set `false` to disable. |
| `ENABLE_CLONE` | `local.enableClone` | CLI `true`, MCP `false` | Both | `ghCloneRepo` and directory fetch. Default differs by surface; set `false` to disable in either. |
| `WORKSPACE_ROOT` | `local.workspaceRoot` | `cwd` | Both | Absolute root for resolving relative local paths. |
| `ALLOWED_PATHS` | `local.allowedPaths` | `[]` | Both | Extra path allowlist for local access; empty means home directory only after validation. |
| `TOOLS_TO_RUN` / `ENABLE_TOOLS` / `DISABLE_TOOLS` | `tools.*` | unset | **MCP** | Whitelist, add to, or remove from the registered tool set. The CLI exposes every tool. |
| `REQUEST_TIMEOUT` | `network.timeout` | `30000` | Both | Request timeout in ms (clamped `5000..300000`). |
| `MAX_RETRIES` | `network.maxRetries` | `3` | Both | Retry attempts (clamped `0..10`). |
| `OCTOCODE_OUTPUT_FORMAT` | `output.format` | `yaml` | Both | Response format: `yaml` or `json`. |

> **Local defaults on; clone differs by surface.** Both CLI and MCP default local tools on; set `ENABLE_LOCAL=false` to disable them. The **CLI** defaults clone on, while the **MCP server** requires `ENABLE_CLONE=true`.

Per-project overrides and custom LSP servers live in a workspace `.octocode/` folder. For the full `.octocoderc` schema, a ready-to-copy example, clone-cache tuning, GitHub Enterprise setup, and precedence details, see the [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md).

---

## Authentication Methods

GitHub-backed tools require authentication. Any one method is enough. Full details: [Authentication Setup](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md).

### Option 1: Octocode CLI (Recommended)

```bash
npx octocode auth login
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

**Full security model, pipeline, and threat coverage: [SECURITY.md](https://github.com/bgauryy/octocode/blob/main/docs/SECURITY.md).** Related: [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) · [Configuration](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) · [Credentials](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md#github-token)

---

## Language Support

Four code-intelligence axes; three are native to the Rust engine and need no external tooling:

| Axis | What it does | How to use it |
|------|--------------|---------------|
| **Structural AST** | Tree-sitter shape queries (`pattern` or YAML `rule`) across 60+ extensions. | `localSearchCode mode:"structural"` · CLI `search --pattern`/`--rule` |
| **Signature outline** | Body-free skeleton with line numbers from real tree-sitter parsing, no heuristics. An anti-growth guard returns the real file when a skeleton wouldn't be smaller. | `minify:"symbols"` · CLI `search <file> --content-view symbols` |
| **Content minification** | Comment/whitespace stripping for 70+ languages and config formats; HTML/Vue/Svelte also minify embedded `<style>`/`<script>`. | `minify:"standard"` (default) |
| **LSP navigation** | definition, references, callers/callees, callHierarchy, hover, typeDefinition, implementation, documentSymbols, via an installed language server; JS/TS also have a native, no-server path. | `lspGetSemantics` · CLI `search --op` / `search --symbols` |

📋 **Full support matrix:** every extension with its exact AST, signature, LSP, and minify capability, machine-generated from the shipped binary, lives in the **[Full format support matrix](https://github.com/bgauryy/octocode/blob/main/docs/LSP_SERVER_LIFECYCLE.md#full-format-support-matrix)** (151 extensions: 61 AST, 47 signature, 32 LSP, 90 minify-only). Regenerate or verify with `yarn workspace @octocodeai/octocode-benchmark matrix:check`.

---

## Skills

> [Agent Skills](https://agentskills.io/what-are-skills) are a lightweight, open format for extending AI agent capabilities.
> Browse and install on [**skills.sh/bgauryy/octocode-mcp**](https://www.skills.sh/bgauryy/octocode-mcp)

**1 skill** under [`skills/`](https://github.com/bgauryy/octocode/tree/main/skills), bundled in the `octocode` package. Start with ⭐ [Research](https://www.skills.sh/bgauryy/octocode-mcp/octocode-research) for evidence-first code work.

```bash
npx octocode skill --list
npx octocode skill --name octocode-research
npx octocode skill --help
```

| Skill | Install | Use when |
|-------|---------|----------|
| ⭐ [**Research**](https://www.skills.sh/bgauryy/octocode-mcp/octocode-research) | `npx octocode skill --name octocode-research` | Evidence-first research, review, refactor, architecture. |

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
| [`packages/octocode-mcp`](https://github.com/bgauryy/octocode/tree/main/packages/octocode-mcp) | `@octocodeai/mcp` | MCP server (stdio) that registers the tool catalog for AI assistants. |
| [`packages/octocode-tools-core`](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core) | `@octocodeai/octocode-tools-core` | Shared tool core: implementations, GitHub client, credentials and token resolution, session, pagination, security bridge. |
| [`packages/octocode-engine`](https://github.com/bgauryy/octocode/tree/main/packages/octocode-engine) | `@octocodeai/octocode-engine` | Rust/napi native engine: security scanning, minification, signatures, structural AST, ripgrep/diff/YAML, LSP. |
| [`packages/octocode-config`](https://github.com/bgauryy/octocode/tree/main/packages/octocode-config) | `@octocodeai/config` | Zero-dep env + config loader: `getOctocodeHome`, `.env` parsing, `.octocoderc` reading. Single source used by every package and skill. |
| [`packages/octocode-vscode`](https://github.com/bgauryy/octocode/tree/main/packages/octocode-vscode) | `octocode-mcp-vscode` | VS Code extension: GitHub OAuth + multi-editor MCP install. |

`packages/octocode-benchmark` (private, not published) holds benchmark methodology, evals, and run artifacts — see [Documentation](#documentation).

---

## Documentation

Website: **[octocode.ai](https://octocode.ai)** · Product docs: **[github.com/bgauryy/octocode/tree/main/docs](https://github.com/bgauryy/octocode/tree/main/docs)**. This section is the canonical documentation index; benchmark methodology, evals, and run artifacts live in [`packages/octocode-benchmark`](https://github.com/bgauryy/octocode/tree/main/packages/octocode-benchmark).

| Area | Docs |
|---|---|
| MCP server | [Octocode MCP Server](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md) · [Configuration](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) · [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) |
| Tools and workflows | [Octocode Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md) · [Octocode Research Skill](https://github.com/bgauryy/octocode/tree/main/skills/octocode-research) · [Search Guide](https://github.com/bgauryy/octocode/blob/main/docs/context/SEARCH_GUIDE.md) |
| CLI and query language | [Octocode CLI Guide](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md) · [Octocode Query Language](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_QUERY_LANGUAGE.md) · [OQL Research Graph Flow](https://github.com/bgauryy/octocode/blob/main/docs/context/OQL_RESEARCH_GRAPH_FLOW.md) |
| Skills | [Skills](https://github.com/bgauryy/octocode/tree/main/skills) |
| Development and security | [Security Model](https://github.com/bgauryy/octocode/blob/main/docs/SECURITY.md) · [LSP Server Lifecycle](https://github.com/bgauryy/octocode/blob/main/docs/LSP_SERVER_LIFECYCLE.md) |
| Benchmarks and evals | [Benchmark Summary](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/BENCHMARK.md) · [Unified CLI/Tool/OQL Eval](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/benchmark/octocode/README.md) · [Benchmark Runbook](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/recipes/agent-benchmark-runbook.md) · [Support Matrix](https://github.com/bgauryy/octocode/blob/main/docs/LSP_SERVER_LIFECYCLE.md#full-format-support-matrix) |
| Shared internals | [Credentials Architecture](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md#github-token) · [Session Persistence](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md#session-persistence) |

---

## Troubleshooting

Having Node.js issues? Run:

```bash
npx node-doctor check --json
```

Read the output and fix accordingly.

---

## Agent Workflows

### Recommended dev mode: Pi + Octocode

[Pi](https://github.com/earendil-works/pi) is a fast, local-first coding agent whose stated philosophy is *"CLI tools with READMEs (Skills) over MCP."* Pairing it with Octocode gives a lean, evidence-driven dev loop — **Pi edits, Octocode researches**. Two routes, pick by how much surface you need:

- **Skill route — recommended, leanest.** Drop the [`octocode-research`](https://www.skills.sh/bgauryy/octocode-mcp/octocode-research) skill into Pi's global skills dir. It drives the Octocode **CLI** directly — no MCP transport, minimal token overhead — and Pi auto-discovers it:

  ```bash
  npx octocode skill --name octocode-research --platform pi
  ```

- **Adapter route — full tool surface.** Install [`pi-mcp-adapter`](https://github.com/nicobailon/pi-mcp-adapter) to expose Octocode MCP tools behind a single ~200-token proxy tool, so servers stay disconnected until a tool is actually called. Enable clone tools with `ENABLE_CLONE=true`.

### Research-driven loop

Most agent failures start before implementation: the agent guesses the owner of a behavior, trusts a snippet without reading the exact source, or edits before proving blast radius. Prefer a cheaper loop first: orient with trees and discovery output, search with Octocode, read exact evidence, use AST/LSP when identity matters, then patch and verify.

That shape keeps the editing surface small while preserving context for what matters: file anchors, symbols, call paths, PR/history evidence, package sources, and the verification command that proves the change. In short, the host edits, Octocode is the map, and skills encode the habit.

### The Manifest

**"Code is Truth, but Context is the Map."** Read the [Manifest of Octocode for Research Driven Development](https://github.com/bgauryy/octocode/blob/main/MANIFEST.md) to understand the philosophy behind Octocode.
