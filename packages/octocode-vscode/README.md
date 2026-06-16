# 🔍🐙 Octocode 

> **Use Octocode. Smart Search. Any Code.**

[Octocode.ai](https://octocode.ai) • [GitHub Repository](https://github.com/bgauryy/octocode-mcp) • [Report Issues](https://github.com/bgauryy/octocode-mcp/issues)

---

## Overview

**Transform GitHub into instant AI knowledge.**

Octocode is the leading AI-powered GitHub code intelligence platform. This extension simplifies the installation of the **Octocode MCP Server** and streamlines authentication with the **`Octocode MCP: Sign in to GitHub`** command. It configures Octocode as a local agent, enabling your AI assistant (Cursor, Antigravity, Windsurf, VS Code Copilot, Claude) to perform deep research, architectural analysis, and pattern discovery across millions of public and private repositories.

## Agenda

- [Requirements](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-vscode/README.md#requirements)
- [Authentication & Installation](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-vscode/README.md#-authentication--installation)
- [Prompts Menu](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-vscode/README.md#prompts-menu)
- [Tools Available](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-vscode/README.md#tools-available)
- [Supported IDEs & AI Assistants](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-vscode/README.md#supported-ides--ai-assistants)
- [Troubleshooting & More Info](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-vscode/README.md#troubleshooting--more-info)

---

## Requirements

- **Node.js**: v20 or higher
- **GitHub Account**: Authorized user

---

## 🔐 Authentication & Installation

Choose the method that works best for your setup.

### Option 1: VS Code OAuth (Quick Start)

**✅ Best for**: Users without CLI who want a quick setup. **First Priority!**

1. Open the **Command Palette** (`Cmd+Shift+P` / `Ctrl+Shift+P`).
2. Run **`Octocode MCP: Sign in to GitHub`**.
3. A popup will appear with a code. **Copy the code**.
4. Click the link to open the GitHub activation page.
5. Paste the code and authorize Octocode.
6. You're all set!

> **Important**: After installation, please check your MCP configuration to ensure the token was set correctly.

### Option 2: GitHub CLI (Recommended for Mac/Linux & Organizations)

**✅ Best for**: Private organizations, SSO, and existing `gh` users.

If you already have the [GitHub CLI (`gh`)](https://cli.github.com/) installed and authenticated, **you are good to go!** Octocode will automatically use your existing credentials.

To set this up:
1. Install `gh` CLI (if not installed).
2. Run `gh auth login` in your terminal.
3. Follow the instructions to authenticate (select "GitHub.com", "HTTPS", "Yes" to authenticate with web browser).

**Configuration (Mac/Linux default):**
```json
{
  "octocode": {
    "command": "npx",
    "args": [
      "octocode-mcp@latest"
    ]
  }
}
```

### Option 3: Manual Token (Recommended for Windows)

**✅ Best for**: Windows users or custom token control.

1. Generate a **GitHub Token** (only `repo` read permission is required):
   - [Classic Token](https://github.com/settings/tokens/new)
   - [Fine-grained Token](https://github.com/settings/personal-access-tokens/new)
2. Add the token to your MCP configuration manually:

**Configuration (Windows/Manual):**
```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["octocode-mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_your_token_here"
      }
    }
  }
}
```

---

## Prompts Menu

Start with `/help` or `/init` to get started!

### `/init`
**Context Gathering**
Interactive interview to establish user role, goals, and constraints. Creates `.octocode/context/context.md`.

### `/research`
**Code Forensics & Discovery**
Deep code discovery, pattern analysis, and bug investigation. Uses parallel bulk queries & staged analysis to prove flows.

### `/plan`
**Adaptive Research & Implementation**
Research, Plan & Implement complex tasks. Breaks down tasks, finds patterns, and guides execution (Interactive/Auto).

### `/generate`
**App Scaffolding**
Research-driven stack selection and project setup. "Measure twice, cut once" approach to new projects.

### `/review_pull_request`
**PR Review** (*Args: `prUrl`*)
Defects-First PR review. Focuses on bugs, security, complexity, and code quality.

### `/review_security`
**Security Audit** (*Args: `repoUrl`*)
Risk assessment and vulnerability surfacing. Maps attack surface and investigates specific risks.

---

## Tools Available

All tools support bulk operations (1–5 queries).

| Tool | Purpose | Key Args |
|------|---------|----------|
| `ghSearchRepos` | **DISCOVER**: Find repos | `stars`, `topicsToSearch` |
| `ghViewRepoStructure` | **EXPLORE**: Map layout | `depth`, `path` |
| `ghSearchCode` | **SEARCH**: Find patterns | `keywordsToSearch`, `match="path"\|"file"` |
| `ghGetFileContent` | **ANALYZE**: Read logic | `matchString`, `startLine` |
| `ghSearchPRs` | **HISTORY**: PR context | `prNumber`, `state`, `diff` |
| `npmSearch` | **DEPS**: Library meta | `query` |

> **Note**: Octocode also supports NPM for smart research and dependency analysis.

---

## Supported IDEs & AI Assistants

This extension works with all major AI-powered editors and assistants:

| IDE | Status | Config Location |
|-----|--------|-----------------|
| **Cursor** | Native MCP | `~/.cursor/mcp.json` |
| **Windsurf** | Native MCP | `~/.codeium/windsurf/mcp_config.json` |
| **Trae** | Native MCP | Platform-specific* |
| **Antigravity** | Native MCP | `~/.gemini/antigravity/mcp_config.json` |
| **VS Code** | Via Claude Desktop | Platform-specific* |

<details>
<summary>*Platform-specific paths</summary>

- **Trae**: macOS: `~/Library/Application Support/Trae/mcp.json` · Windows: `%APPDATA%/Trae/mcp.json` · Linux: `~/.config/Trae/mcp.json`
- **VS Code**: Falls back to Claude Desktop config · macOS: `~/Library/Application Support/Claude/claude_desktop_config.json` · Windows: `%APPDATA%/Claude/claude_desktop_config.json` · Linux: `~/.config/Claude/claude_desktop_config.json`

</details>


## Documentation

- [Docs Index](https://github.com/bgauryy/octocode-mcp/blob/main/docs/README.md) — all Octocode docs
- [Authentication Setup](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md) — GitHub auth model
- [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md) — MCP env/config options

## Troubleshooting & More Info

For more details on features, prompts, tutorials, and documentation, visit **[octocode.ai](https://octocode.ai)**.

For installation help: **[https://octocode.ai/?auth=cli#installation](https://octocode.ai/?auth=cli#installation)**

---

## Privacy & Telemetry

Octocode collects **de-identified** telemetry data to improve the tool, including command usage and error rates. We **never** collect source code, environment variables, or PII.

You can opt-out at any time:

```bash
export LOG=false
```

For full details, please read our [Privacy Policy](https://github.com/bgauryy/octocode-mcp/blob/main/PRIVACY.md) and [Terms of Usage](https://github.com/bgauryy/octocode-mcp/blob/main/TERMS.md).

---

## License

This project is licensed under the **MIT License**.

Copyright © 2026 Octocode AI.

See [LICENSE](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-vscode/LICENSE) for details.

<div align="center">
  <p>Powered by <a href="https://octocode.ai"><b>Octocode.ai</b></a></p>
</div>
