# Octocode CLI

<div align="center">

[![npm version](https://img.shields.io/npm/v/octocode-cli.svg?style=flat-square)](https://www.npmjs.com/package/octocode-cli)
[![npm downloads](https://img.shields.io/npm/dm/octocode-cli.svg?style=flat-square)](https://www.npmjs.com/package/octocode-cli)
[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-cli/LICENSE)

**Code research from the terminal:** run Octocode tools for local workspaces, GitHub repositories, LSP navigation, packages, and PRs. It also configures Octocode MCP for supported IDEs.

[Website](https://octocode.ai) · [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md) · [GitHub](https://github.com/bgauryy/octocode-mcp)

  <img src="https://github.com/bgauryy/octocode-mcp/raw/main/packages/octocode-mcp/assets/logo_white.png" width="400px" alt="Octocode Logo">


</div>

---

## Install

```bash
# Homebrew (macOS / Linux): installs the `octocode` command globally
brew install bgauryy/octocode/octocode

# or tap once, then use the short name
brew tap bgauryy/octocode
brew install octocode

# or npm (global)
npm install -g octocode-cli

# or run once without installing
npx octocode-cli install
```

Verify, then sign in:

```bash
octocode --version        # → octocode v1.6.0
octocode login            # GitHub OAuth (device flow)
```

The installed command is **`octocode`** (not `octocode-cli`). Homebrew pulls in Node automatically.

## Quick Start

```bash
octocode context                                       # agent protocol + tool fields
octocode tree .                                        # inspect local structure
octocode files auth . --search both --ext ts           # discover files + content hits
octocode search "TODO" . --type ts                     # search code
octocode repo mcp agents --language TypeScript         # discover GitHub repos
octocode pkg zod                                       # package metadata + source repo
octocode symbols src/index.ts                          # semantic outline
octocode lsp src/index.ts --type documentSymbols       # semantic outline
octocode tools localSearchCode --scheme                # inspect exact schema
octocode tools localSearchCode --queries '{"path":".","keywords":"TODO"}'
```

> **Agents:** run `octocode context` first. It prints the protocol, every tool, and input field specs in one shot.

---

## The 12 Tools

Call any tool directly from the terminal. Great for scripts, pipelines, and one-off queries. No MCP server required.

| Group | Tool | What it does |
|-------|------|--------------|
| **GitHub** | `ghSearchCode` | Search code across GitHub `[EXTERNAL]` |
| | `ghSearchRepos` | Search repositories by keywords/topics `[EXTERNAL]` |
| | `ghSearchPRs` | Search pull requests `[EXTERNAL]` |
| | `ghGetFileContent` | Read file content (matchString, line ranges) `[EXTERNAL]` |
| | `ghViewRepoStructure` | List a repo's directory tree `[EXTERNAL]` |
| | `ghCloneRepo` | Clone a repo/subtree to disk for local + LSP analysis |
| **Local** | `localSearchCode` | Search code patterns with ripgrep (PCRE2) |
| | `localFindFiles` | Find files by name/metadata |
| | `localGetFileContent` | Read local file content |
| | `localViewStructure` | View a local directory tree |
| **LSP** | `lspGetSemantics` | Definitions, references, call flow, hover, symbols, type definitions, implementations |
| **Package** | `npmSearch` | Resolve an npm package to its source repo + metadata |

```bash
# Discover
octocode context                                  # agent bootstrap: protocol + tools + input fields
octocode context --full                           # …plus every tool's full JSON schema inline
octocode tools                                    # list all tools
octocode tools localSearchCode --scheme           # show one tool's schema
octocode tools localSearchCode ghSearchCode   # batch schemas

# Run
octocode tools localSearchCode --queries '{"path":".","keywords":"TODO"}'
octocode tools ghSearchCode --queries '{"keywordsToSearch":["useReducer"],"owner":"facebook","repo":"react"}'
octocode tools localSearchCode --queries '{"path":".","keywords":"TODO"}' --json     # full MCP envelope
octocode tools localSearchCode --queries '{"path":".","keywords":"TODO"}' --compact  # leanest (structuredContent only)
```

The shared metadata fields (`id`, `researchGoal`, `reasoning`, `mainResearchGoal`) are auto-filled. Provide only tool-specific fields.

> **For agents:** run `octocode context` once for the full bootstrap (protocol + every tool + input fields + the mandatory "read the schema before calling" rule + the exit-code table). Then `octocode tools <name> --scheme` to confirm a tool's exact schema before calling it. This checklist is also printed at the top of `octocode --help`.

---

## GitHub Token

Octocode resolves a GitHub token in this order: **environment variable → Octocode-managed token (`octocode login`) → `gh` CLI**.

For env-based setup, any of these are accepted (checked in this priority): `OCTOCODE_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`.

```bash
export GITHUB_TOKEN=ghp_xxx        # or OCTOCODE_TOKEN / GH_TOKEN
octocode token --source           # show which token & source is being used
octocode token --validate         # ping the GitHub API to verify it
octocode token                    # masked on screen; raw when piped, e.g. export GH_TOKEN=$(octocode token)
octocode token --reveal           # print the full token on screen
```

---

## Commands

| Command | What it does |
|---------|--------------|
| `get <path\|github-ref>` | Fetch and minify file content |
| `tree <path\|github-ref>` | View local or GitHub directory structure |
| `files <query> [path\|github-ref]` | Find file paths and content matches |
| `search <pattern> <path\|github-ref>` | Search local or GitHub code |
| `pr <owner/repo[#N] \| PR-URL>` | Search or deep-dive pull requests |
| `repo <keywords...>` | Discover GitHub repositories by keyword, topic, owner, and quality filters |
| `pkg <package>` | Research npm package metadata and source repository |
| `symbols <file\|path>` | Show semantic symbol outlines for local files or directories |
| `lsp <file> --type <type>` | Run LSP semantic navigation for a local source file |
| `tools [name...]` | List tools, show schema(s), or run with `--queries '<json>'` |
| `context` | Print agent context and every tool schema |
| `install --ide <ide>` | Configure octocode-mcp for an IDE |
| `auth [login\|logout\|status\|token\|refresh]` | GitHub authentication |
| `login` / `logout` | Top-level shortcuts for auth login/logout |
| `token` | Print the resolved GitHub token |
| `status` | Health check: auth, MCP clients, and cache |
| `status --sync` | Include per-MCP sync analysis |
| `skills` | Install, remove, list, search, read, or sync skills |

Most subcommands accept `--hostname <host>` for GitHub Enterprise.

---

## Global Flags

| Flag | Description |
|------|-------------|
| `--json` | Raw JSON output (full MCP envelope) for tool runs |
| `--compact` | Leanest tool output: minified `structuredContent` only (~60% smaller than `--json`) |
| `--no-color` | Disable ANSI colors (also honored via `NO_COLOR=1`) |
| `--version` | Print the CLI version |
| `--help` | Show help for the CLI or a command |

## Environment

| Variable | Purpose |
|----------|---------|
| `OCTOCODE_TOKEN` | GitHub token (checked first) |
| `GH_TOKEN` | GitHub token (checked second) |
| `GITHUB_TOKEN` | GitHub token (checked third) |
| `OCTOCODE_HOME` | Override the data directory (default `~/.octocode`) |

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | General error |
| `2` | Invalid input / unsupported flags |
| `3` | Unknown tool or command |
| `4` | Authentication failure |
| `5` | Tool / API execution error |
| `7` | Rate limited |

Typed codes `2`–`7` apply to the tool surface and command dispatch, so agents can branch on the failure mode without parsing output. Management commands (`install`, `auth`, `skills`, …) use `0`/`1`.

---

## Supported Clients

`cursor`, `claude-desktop`, `claude-code`, `windsurf`, `zed`, `vscode-cline`, `vscode-roo`, `vscode-continue`, `opencode`, `trae`, `antigravity`, `codex`, `gemini-cli`, `goose`, `kiro`

---

## Troubleshooting

```bash
octocode status --sync                   # Full health check including sync analysis
octocode token --validate                # Verify your GitHub token against the API
octocode token --source                  # Debug token resolution chain
octocode install --ide cursor --check    # Pre-flight before installing
```

---

## Docs

- [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md): full command and flag reference
- [Skills Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/SKILLS_GUIDE.md): bundled skills installation
- [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md)

---

## Privacy & Telemetry

De-identified telemetry (command usage, error rates) helps improve the CLI. Source code, env values, and repo contents are not collected.

[Privacy Policy](https://github.com/bgauryy/octocode-mcp/blob/main/PRIVACY.md) · [Terms of Usage](https://github.com/bgauryy/octocode-mcp/blob/main/TERMS.md)

MIT. Copyright 2026 Octocode AI.
