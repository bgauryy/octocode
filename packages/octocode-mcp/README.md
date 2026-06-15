<div align="center">
  <img src="https://github.com/bgauryy/octocode-mcp/raw/main/packages/octocode-mcp/assets/logo_white.png" width="400px" alt="Octocode Logo">

  # Octocode MCP

  **Intelligent Code Context for AI Systems**

  A Model Context Protocol (MCP) server enabling AI assistants to search, analyze, and extract insights from millions of GitHub repositories with enterprise-grade security and token efficiency.

  [![MCP Community Server](https://img.shields.io/badge/Model_Context_Protocol-Official_Community_Server-blue?style=flat-square)](https://github.com/modelcontextprotocol/servers)
  [![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/bgauryy/octocode-mcp)
  [![Trust Score](https://archestra.ai/mcp-catalog/api/badge/quality/bgauryy/octocode-mcp)](https://archestra.ai/mcp-catalog/bgauryy__octocode-mcp)

  <a href="https://octocode.ai"><img src="https://img.shields.io/badge/Website-007ACC?style=for-the-badge&logo=link&logoColor=white" alt="Website"></a>
  <a href="https://www.youtube.com/@Octocode-ai"><img src="https://img.shields.io/badge/YouTube-FF0000?style=for-the-badge&logo=youtube&logoColor=white" alt="YouTube Channel"></a>

</div>

---

<div align="center">

### ✨ Featured On

[![MCP Official Servers](https://img.shields.io/badge/MCP-Official%20Community%20Server-007ACC?style=for-the-badge&logo=github&logoColor=white)](https://github.com/modelcontextprotocol/servers)
[![Awesome MCP Servers](https://img.shields.io/badge/Awesome-MCP%20Servers-FF6B6B?style=for-the-badge&logo=github&logoColor=white)](https://github.com/punkpeye/awesome-mcp-servers)

</div>

---

## Table of Contents

- [See It In Action](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-mcp/README.md#see-it-in-action)
- [Installation](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-mcp/README.md#installation)
- [More Examples](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-mcp/README.md#more-examples)
- [Overview](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-mcp/README.md#overview)
- [Tools](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-mcp/README.md#tools)
- [Documentation](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-mcp/README.md#documentation)
- [Community](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-mcp/README.md#community)
- [License](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-mcp/README.md#license)

---

## See It In Action

### Full-Stack Application Built in Under 10 Minutes

Watch AI assistant use Octocode to research, plan, and build a complete chat application with Express backend.

**Prompt:**

> **Use Octocode MCP for Deep Research**
>
> I want to build an application with chat (front-end) that shows a chat window to the user.
> The user enters a prompt in the chat, and the application sends the prompt to an Express backend that uses AI to process the request.
>
> Add a return box (to show the message returned from the AI) and loaders to the UI.
> I want to build an AI agent system in Node.js using LangChain and LangGraph. Can you research the latest patterns?
>
> Please conduct thorough research on how to create this in the best way possible.
> Focus on repositories with good documentation and recent activity.
>
> - Do a deep research
> - Create a plan document
> - Initiate the plan and create the application

**Phase 1: Research & Planning**

https://github.com/user-attachments/assets/4225ab98-ae2f-46dc-b3ce-7d117e552b8c

[Octocode Plan Document](https://gist.github.com/bgauryy/06504671c0d5fef727fe22c492e054d6) - Detailed architecture and step-by-step guide

**Phase 2: Implementation**

https://github.com/user-attachments/assets/2aaee9f1-3592-438a-a633-255b5cbbb8e1

**Result**: Production-ready full-stack application with authentication, real-time features, and best practices - **All in less than 10 minutes**

---

### Research and Build Fullstack Agentic Application with /research command in Under 10 Minutes

**Why use the `/research` command?** Instead of manually searching through repositories and piecing together information, let the AI conduct comprehensive research for you:

- **🎯 Intelligent Tool Orchestration**: Automatically selects and combines the right Octocode tools (repository search, code search, file content, PR analysis, repo structure) based on your research needs
- **🧠 Smart Decision Making**: Makes strategic choices throughout the research flow—when to search broadly vs. specifically, which repositories to explore, and how to validate findings
- **👥 Multi-Purpose Research**: Perfect for feature discovery (product managers), code understanding (developers), bug investigation, flow analysis, planning from scratch, dependency tracking, security audits, and more
- **🔬 Specialized Workflows**: Handles Technical Research (code flows), Product Research (docs+code validation), Pattern Analysis (cross-repo comparison), Bug Investigation, Architecture Mapping, API Research, Security/Auth flows, and more
- **🔍 Transparent Reasoning**: Shows you exactly which tools it's using, what it's searching for, and why at each step
- **🎨 Adaptive Strategy**: Works across public repos, private organizations, and specific repositories with configurable depth (overview, deep dive, or cross-repo comparison)
- **📊 Cross-Validated Results**: Leverages multiple Octocode tools to verify information from different sources and perspectives
- **🚀 Actionable Insights**: Delivers implementation-ready plans with code examples, not just raw information

**Prompt:**

> /octocode/research How can I use LangChain, LangGraph, and similar open-source AI tools to create agentic 
> flows between agents for goal-oriented tasks?
> Can you suggest UI frameworks I can use to build a full-stack AI application?

https://github.com/user-attachments/assets/82ed97ae-57a9-46ae-9acd-828a509e711b

---

### Discover APIs, Frameworks, and Dive Into Internal Implementation Details

Octocode excels at both **broad discovery** and **deep code analysis**. Whether you're exploring new APIs, finding frameworks, or understanding how popular libraries work under the hood, Octocode provides comprehensive answers in seconds.

**First Prompt - Broad Discovery:**

> list top repositories for:
>
> - Stock market APIs (Typescript)
> - Cursor rules examples
> - UI for AI
> - Mobile development using React
> - State management for React

**What happens:** Octocode searches across GitHub to find the most popular and well-maintained repositories for each category, analyzing stars, activity, documentation quality, and recent updates. You get curated lists with context about each repository's strengths.

**Second Prompt - Deep Implementation Analysis:**

> How React implemented useState under the hood?

**What happens:** Octocode dives into React's source code, traces the implementation flow, analyzes the relevant files (ReactHooks.js, ReactFiberHooks.js), and explains the internal mechanics including fiber architecture, hook state management, and dispatcher patterns—all with code references and detailed explanations.

**The Power:** Move seamlessly from **discovering what exists** to **understanding how it works** in a single conversation. No manual repository hunting or code spelunking required.

https://github.com/user-attachments/assets/c184d5d4-c9b6-40a1-a55a-41cb9b3ecc4f

---

## Agentic Minification Flow

Octocode optimizes for code-research quality, not just the smallest response. Agents can start compressed, drill into the exact slice, and switch to raw text when evidence matters.

| View | Use when | Why |
|------|----------|-----|
| `minify:"standard"` | Default file read | Removes low-signal noise while keeping readable code shape |
| `minify:"symbols"` | Unknown or large file | Maps imports, classes, functions, selectors, and SQL objects with line numbers |
| `startLine`/`endLine` or `matchString` | Known body or search hit | Opens only the relevant slice |
| `charOffset`/`charLength` | A response is paginated | Continues the same view without rereading everything |
| `minify:"none"` | Quotes, comments, diffs, review findings, tests | Returns exact raw text for line-accurate evidence |

Recommended loop: `symbols` → focused slice → `none` only for proof. Skip straight to `matchString` or `startLine`/`endLine` when you already know the target.

---

## Installation

### ⚡ Quick Start (Recommended)

Install and configure Octocode with the interactive CLI:

```bash
npx octocode-cli install
```

This will automatically:
- Detect your installed IDEs (Cursor, Claude, Windsurf, etc.)
- Verify your environment (Node.js, GitHub CLI)
- Configure the MCP server correctly for your selected client

---

### Standalone Binary (No Node.js Required)

```bash
curl -fsSL https://raw.githubusercontent.com/bgauryy/octocode-mcp/main/install/install.sh | sh
```

See the [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md#quick-checks) for quick setup checks.

---

### npm / npx (Requires Node.js)

#### Prerequisites

- **Node.js** >= 20.0.0
- **GitHub Authentication** (choose one):
  - **GitHub CLI (recommended)**: Install from [cli.github.com](https://cli.github.com/) and run `gh auth login`
  - **Personal Access Token**: Create at [github.com/settings/tokens](https://github.com/settings/tokens) with scopes: `repo`, `read:user`, `read:org`

### Getting started

First, install the Octocode MCP server with your client.

**Standard config** works in most of the tools:

```js
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": [
        "octocode-mcp@latest"
      ]
    }
  }
}
```

> **Note**: This configuration uses GitHub CLI authentication. For Personal Access Token, see the [Authentication Guide](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-mcp/README.md#authentication-methods) below.

[<img src="https://img.shields.io/badge/VS_Code-VS_Code?style=flat-square&label=Install%20Server&color=0098FF" alt="Install in VS Code">](https://insiders.vscode.dev/redirect?url=vscode%3Amcp%2Finstall%3F%257B%2522name%2522%253A%2522octocode%2522%252C%2522command%2522%253A%2522npx%2522%252C%2522args%2522%253A%255B%2522octocode-mcp%2540latest%255D%257D) [<img alt="Install in VS Code Insiders" src="https://img.shields.io/badge/VS_Code_Insiders-VS_Code_Insiders?style=flat-square&label=Install%20Server&color=24bfa5">](https://insiders.vscode.dev/redirect?url=vscode-insiders%3Amcp%2Finstall%3F%257B%2522name%2522%253A%2522octocode%2522%252C%2522command%2522%253A%2522npx%2522%252C%2522args%2522%253A%255B%2522octocode-mcp%2540latest%255D%257D)

<details>
<summary>Amp</summary>

Add via the Amp VS Code extension settings screen or by updating your settings.json file:

```json
"amp.mcpServers": {
  "octocode": {
    "command": "npx",
    "args": [
      "octocode-mcp@latest"
    ]
  }
}
```

**Amp CLI Setup:**

Add via the `amp mcp add` command below:

```bash
amp mcp add octocode -- npx octocode-mcp@latest
```

</details>

<details>
<summary>Claude Code</summary>

Use the Claude Code CLI to add the Octocode MCP server:

```bash
claude mcp add octocode -- npx octocode-mcp@latest
```

**With local tools enabled:**

```bash
claude mcp add octocode -e ENABLE_LOCAL=true -- npx octocode-mcp@latest
```

</details>

<details>
<summary>Claude Desktop</summary>

Follow the MCP install [guide](https://modelcontextprotocol.io/quickstart/user), use the standard config above.

</details>

<details>
<summary>Codex</summary>

Use the Codex CLI to add the Octocode MCP server:

```bash
codex mcp add octocode npx "octocode-mcp@latest"
```

Alternatively, create or edit the configuration file `~/.codex/config.toml` and add:

```toml
[mcp_servers.octocode]
command = "npx"
args = ["octocode-mcp@latest"]
```

For more information, see the [Codex MCP documentation](https://github.com/openai/codex/blob/main/codex-rs/config.md#mcp_servers).

</details>

<details>
<summary>Cursor</summary>

#### Click the button to install:

[<img src="https://cursor.com/deeplink/mcp-install-dark.svg" alt="Install in Cursor">](https://cursor.com/en/install-mcp?name=octocode&config=eyJjb21tYW5kIjoibnB4IiwiYXJncyI6WyJvY3RvY29kZS1tY3BAbGF0ZXN0Il19)

#### Or install manually:

Go to `Cursor Settings` -> `MCP` -> `Add new MCP Server`. Name to your liking, use `command` type with the command `npx octocode-mcp@latest`. You can also verify config or add command like arguments via clicking `Edit`.

#### Project-Specific Configuration

Create `.cursor/mcp.json` in your project root:

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

</details>

<details>
<summary>Cline</summary>

Add via the Cline VS Code extension settings or by updating your `cline_mcp_settings.json` file:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": [
        "octocode-mcp@latest"
      ]
    }
  }
}
```

</details>

<details>
<summary>Gemini CLI</summary>

Follow the MCP install [guide](https://github.com/google-gemini/gemini-cli/blob/main/docs/tools/mcp-server.md#configure-the-mcp-server-in-settingsjson), use the standard config above.

</details>

<details>
<summary>Goose</summary>

#### Click the button to install:

[![Install in Goose](https://block.github.io/goose/img/extension-install-dark.svg)](https://block.github.io/goose/extension?cmd=npx&arg=octocode-mcp%40latest&id=octocode&name=Octocode&description=Intelligent%20code%20research%20and%20GitHub%20repository%20analysis)

#### Or install manually:

Go to `Advanced settings` -> `Extensions` -> `Add custom extension`. Name to your liking, use type `STDIO`, and set the `command` to `npx octocode-mcp@latest`. Click "Add Extension".

</details>

<details>
<summary>Kiro</summary>

Follow the MCP Servers [documentation](https://kiro.dev/docs/mcp/). For example in `.kiro/settings/mcp.json`:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": [
        "octocode-mcp@latest"
      ]
    }
  }
}
```

</details>

<details>
<summary>LM Studio</summary>

#### Click the button to install:

[![Add MCP Server octocode to LM Studio](https://files.lmstudio.ai/deeplink/mcp-install-light.svg)](https://lmstudio.ai/install-mcp?name=octocode&config=eyJjb21tYW5kIjoibnB4IiwiYXJncyI6WyJvY3RvY29kZS1tY3BAbGF0ZXN0Il19)

#### Or install manually:

Go to `Program` in the right sidebar -> `Install` -> `Edit mcp.json`. Use the standard config above.

</details>

<details>
<summary>opencode</summary>

Follow the MCP Servers [documentation](https://opencode.ai/docs/mcp-servers/). For example in `~/.config/opencode/opencode.json`:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "octocode": {
      "type": "local",
      "command": [
        "npx",
        "octocode-mcp@latest"
      ],
      "enabled": true
    }
  }
}
```

</details>

<details>
<summary>Qodo Gen</summary>

Open [Qodo Gen](https://docs.qodo.ai/qodo-documentation/qodo-gen) chat panel in VSCode or IntelliJ → Connect more tools → + Add new MCP → Paste the standard config above.

Click <code>Save</code>.

</details>

<details>
<summary>VS Code</summary>

#### Click the button to install:

[<img src="https://img.shields.io/badge/VS_Code-VS_Code?style=flat-square&label=Install%20Server&color=0098FF" alt="Install in VS Code">](https://insiders.vscode.dev/redirect?url=vscode%3Amcp%2Finstall%3F%257B%2522name%2522%253A%2522octocode%2522%252C%2522command%2522%253A%2522npx%2522%252C%2522args%2522%253A%255B%2522octocode-mcp%2540latest%255D%257D) [<img alt="Install in VS Code Insiders" src="https://img.shields.io/badge/VS_Code_Insiders-VS_Code_Insiders?style=flat-square&label=Install%20Server&color=24bfa5">](https://insiders.vscode.dev/redirect?url=vscode-insiders%3Amcp%2Finstall%3F%257B%2522name%2522%253A%2522octocode%2522%252C%2522command%2522%253A%2522npx%2522%252C%2522args%2522%253A%255B%2522octocode-mcp%2540latest%255D%257D)

#### Or install manually:

Follow the MCP install [guide](https://code.visualstudio.com/docs/copilot/chat/mcp-servers#_add-an-mcp-server), use the standard config above. You can also install the Octocode MCP server using the VS Code CLI:

```bash
# For VS Code
code --add-mcp '{"name":"octocode","command":"npx","args":["octocode-mcp@latest"]}'
```

After installation, the Octocode MCP server will be available for use with your GitHub Copilot agent in VS Code.

</details>

<details>
<summary>Warp</summary>

Go to `Settings` -> `AI` -> `Manage MCP Servers` -> `+ Add` to [add an MCP Server](https://docs.warp.dev/knowledge-and-collaboration/mcp#adding-an-mcp-server). Use the standard config above.

Alternatively, use the slash command `/add-mcp` in the Warp prompt and paste the standard config from above:

```js
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": [
        "octocode-mcp@latest"
      ]
    }
  }
}
```

</details>

<details>
<summary>Windsurf</summary>

Follow Windsurf MCP [documentation](https://docs.windsurf.com/windsurf/cascade/mcp). Use the standard config above.

</details>

<details>
<summary>Zed</summary>

Follow the MCP Servers [documentation](https://zed.dev/docs/assistant/model-context-protocol). Use the standard config above.

</details>

---

### Authentication Methods

Octocode MCP supports two authentication methods:

#### Option 1: GitHub CLI (Recommended)

**Advantages**: Automatic token management, works with 2FA, supports SSO

```bash
# Install GitHub CLI
# macOS
brew install gh

# Windows
winget install --id GitHub.cli

# Linux
# See https://github.com/cli/cli/blob/trunk/docs/install_linux.md

# Authenticate
gh auth login
```

Then use the standard configuration (no `GITHUB_TOKEN` needed).

#### Option 2: Personal Access Token

**When to use**: CI/CD environments, automation, or if GitHub CLI isn't available

1. Create a token at [github.com/settings/tokens](https://github.com/settings/tokens)
2. Select scopes: `repo`, `read:user`, `read:org`
3. Add to your MCP configuration:

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

> **Security Tip**: Never commit tokens to version control. Use environment variables or secure secret management.

---

### Verify Installation

After installation, verify Octocode MCP is working:

1. **Restart your MCP client** completely
2. **Check connection status**:
   - **Cursor**: Look for green dot in Settings → Tools & Integrations → MCP Tools
   - **Claude Desktop**: Check for "octocode" in available tools
   - **VS Code**: Verify in GitHub Copilot settings
3. **Test with a simple query**:
   ```
   Search GitHub for React hooks implementations
   ```

If you see Octocode tools being used, you're all set! 🎉

---

## GitHub Enterprise Support

Octocode MCP supports GitHub Enterprise Server instances with custom API URLs.

### Configuration

Add the `GITHUB_API_URL` environment variable to your MCP configuration:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["octocode-mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "your_token",
        "GITHUB_API_URL": "https://github.company.com/api/v3"
      }
    }
  }
}
```

**Default:** If not specified, defaults to `https://api.github.com` (public GitHub).

**Note:** Ensure your GitHub Enterprise token has the same scopes as documented in the [Authentication Setup](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md).

---

## More Examples

### Additional Demonstrations

#### ThreeJS Implementation Quality Comparison

**[Interactive Demo](https://octocode-sonnet4-gpt5-comparisson.vercel.app/)**

Side-by-side comparison showing:
- **Generic AI**: Basic implementation with common patterns
- **Octocode-Enhanced AI**: Production-grade implementation with advanced techniques from real projects

**Key Differences**:
- Performance optimizations from high-performance projects
- Proper resource management patterns
- Industry-standard error handling
- Real-world edge case handling

#### Deep Technical Research

**[YouTube: React Hooks Internals](https://www.youtube.com/watch?v=BCOpsRjAPU4&t=9s)**

Demonstrates progressive research workflow:
1. Repository discovery (React source)
2. Structure exploration (hooks implementation)
3. Code analysis (internal mechanisms)
4. Comprehensive explanation with code references

---

## Documentation

### Comprehensive Guides

| Resource | Description | Link |
|----------|-------------|------|
| **Official Website** | Interactive tutorials, demos, community | [octocode.ai](https://octocode.ai) |
| **Docs Index** | All configuration, development, workflow, and reference docs | [docs/README.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/README.md) |
| **Configuration Guide** | Environment variables and server configuration | [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md) |
| **Authentication Guide** | Setup instructions for GitHub | [Authentication Setup](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md) |
| **Remote Tool Reference** | GitHub tool behavior and schemas | [GitHub Tools](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/GITHUB_TOOLS_REFERENCE.md) |
| **Local Tool Reference** | Local filesystem search, metadata, and content tools | [Local Tools](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LOCAL_TOOLS_REFERENCE.md) |
| **LSP Tool Reference** | Semantic navigation and diagnostics tools | [LSP Tools](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LSP_TOOLS_REFERENCE.md) |
| **Clone Workflow** | Clone GitHub repos, then analyze locally with LSP | [Clone & Local Tools Workflow](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/workflows/CLONE_AND_LOCAL_TOOLS_WORKFLOW.md) |
| **YouTube Channel** | Video tutorials and demonstrations | [Octocode on YouTube](https://www.youtube.com/@Octocode-ai) |

---

## Community

### Get Support

- **GitHub Discussions**: [Ask questions, share ideas](https://github.com/bgauryy/octocode-mcp/discussions)
- **GitHub Issues**: [Report bugs, request features](https://github.com/bgauryy/octocode-mcp/issues)
- **Documentation**: [Complete guides and references](https://octocode.ai)
- **YouTube**: [Video tutorials and examples](https://www.youtube.com/@Octocode-ai)

### Show Your Support

If Octocode helps your AI development workflow:

- **Star the repository** on [GitHub](https://github.com/bgauryy/octocode-mcp)
- **Share on social media** with #OctocodeMCP
- **Write about your experience** on your blog
- **Create tutorials** and share with the community
- **Contribute** improvements and bug fixes

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

See [LICENSE](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-mcp/LICENSE) for details.
