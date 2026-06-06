# Octocode: Research Driven Development for AI

<div align="center">
  <img src="https://github.com/bgauryy/octocode-mcp/raw/main/packages/octocode-mcp/assets/logo_white.png" width="400px" alt="Octocode Logo">
  
  <h3>Research like a Senior Staff Engineer.<br/>In every codebase, in seconds.</h3>
  <p><strong>Stop guessing.</strong> Octocode researches code <strong>locally and externally</strong>: your own workspace (ripgrep + LSP-level go-to-definition, references, call hierarchy) and the world's (GitHub repos, PRs, npm/PyPI packages), turning it into verifiable evidence your AI can search, read, and trace.</p>
  <p>Use it as an <strong>MCP server</strong> inside your AI assistant, or as a <strong>terminal CLI</strong>.</p>

  <p>
    <a href="https://octocode.ai"><strong>octocode.ai</strong></a>
    &nbsp;·&nbsp;
    <a href="#-as-an-mcp-server">MCP Server</a>
    &nbsp;·&nbsp;
    <a href="#-as-a-cli">CLI</a>
    &nbsp;·&nbsp;
    <a href="#skills">Skills</a>
  </p>
</div>

---

## Two ways to run Octocode

| | 🔌 **As an MCP Server** | 💻 **As a CLI** |
|---|---|---|
| **For** | Your AI assistant (Claude Code, Cursor, Claude Desktop, +13 more) | Your terminal & scripts |
| **Install** | `npx octocode-cli install` | `brew install bgauryy/octocode/octocode` |
| **You get** | 14 research tools wired into your agent | The same 14 tools, runnable from the shell |
| **Best for** | Deep agent research, planning, PR review | Quick scripted lookups, CI, piping to other tools |

Same engine, same tools, two surfaces. Pick one or use both.

> **Prerequisites**: GitHub authentication for the GitHub-backed tools. Run `octocode login`, or see [Authentication Setup](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md).

---

## 🔌 As an MCP Server

Wire Octocode's tools into your AI assistant. The interactive installer handles GitHub OAuth, MCP server config, and the skills marketplace:

```bash
npx octocode-cli install
```

Pass `--ide <client>` for a non-interactive install (e.g. `octocode install --ide cursor`), and `-m direct` only to point a client at a locally installed MCP binary.

<details>
<summary><strong>One-Click Install (Cursor)</strong></summary>

[<img src="https://cursor.com/deeplink/mcp-install-dark.svg" alt="Install in Cursor">](https://cursor.com/en/install-mcp?name=octocode&config=eyJjb21tYW5kIjoibnB4IiwiYXJncyI6WyJvY3RvY29kZS1tY3BAbGF0ZXN0Il19)

</details>

<details>
<summary><strong>Manual MCP Configuration</strong></summary>

Add to your MCP configuration file:

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
<summary><strong>Research Skill (Direct Install)</strong></summary>

```bash
npx add-skill https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-research
```

</details>

The [Octocode MCP Server](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-mcp) connects your AI assistant to code:

- **GitHub**: Search repositories, find usage patterns, read implementations, explore PRs
- **Local Tools**: Search code (ripgrep), browse directories, find files in your local codebase
- **LSP Intelligence**: Go to Definition, Find References, Call Hierarchy (compiler-level understanding)
- **Package Discovery**: Resolve npm/PyPI packages to their source repos

https://github.com/user-attachments/assets/de8d14c0-2ead-46ed-895e-09144c9b5071

---

## 💻 As a CLI

Install the `octocode` command globally with [Homebrew](https://brew.sh) and run all 14 tools straight from your terminal, no MCP wiring required:

```bash
brew install bgauryy/octocode/octocode    # → octocode v1.5.0
```

> Or `brew tap bgauryy/octocode && brew install octocode` (short form), or run on demand with `npx octocode-cli`. Node is pulled in automatically. Run `octocode login` first for the GitHub-backed tools.

The CLI is both a **setup wizard** (install the MCP server + skills, manage GitHub auth, sync configs across editors) and a **standalone tool runner** (call any Octocode tool, pipe the JSON anywhere).

**Commands**

| Command | What it does |
|---------|--------------|
| `octocode install` | Configure `octocode-mcp` for an IDE/agent (`--ide <client>`, `-m npx\|direct`, `--force`, `--json`) |
| `octocode auth` | Manage GitHub authentication (interactive menu) |
| `octocode login` / `logout` | Sign in / out of GitHub via OAuth device flow (`--hostname` for Enterprise) |
| `octocode status` | Octocode health: auth + installed MCPs + cache (`--sync`, `--json`) |
| `octocode token` | Print the GitHub token using the MCP server's resolution order (`--source`, `--validate`) |
| `octocode skills` | Search / install / remove / sync Agent Skills (`--targets`, `--mode copy\|symlink`) |
| `octocode mcp` | MCP marketplace: `list` / `install` / `remove` / `status` (`--id`, `--client`, `--env`) |
| `octocode sync` | Sync MCP configs across all installed IDE clients (`--dry-run`, `--status`) |
| `octocode cache` | Inspect / clean cloned repos, skills, logs, and tool caches |
| `octocode tools` | List tools, show a tool's schema, or run one with `--queries '<json>'` |
| `octocode instructions` | Print MCP instructions + every tool schema |

Top-level flags: `--version`/`-v`, `--help`/`-h`, `--json`/`-j`.

**The 14 tools.** Run any directly with `octocode tools <name> --queries '<json>'`:

| Group | Tools |
|-------|-------|
| GitHub | `githubSearchCode` · `githubSearchRepositories` · `githubSearchPullRequests` · `githubGetFileContent` · `githubViewRepoStructure` · `githubCloneRepo` |
| Local | `localSearchCode` (ripgrep) · `localFindFiles` · `localGetFileContent` · `localViewStructure` |
| LSP | `lspGotoDefinition` · `lspFindReferences` · `lspCallHierarchy` |
| Package | `packageSearch` (npm / PyPI → source repo) |

**Quick start**

```bash
octocode login                                   # GitHub OAuth
octocode install --ide cursor                    # wire MCP into an editor
octocode skills install --targets claude-code    # add Agent Skills
octocode tools                                    # list every tool
octocode tools localSearchCode --queries '{"path":".","pattern":"fn"}'
```

Install targets include Cursor, Claude Code, Claude Desktop, Windsurf, Zed, Trae, Antigravity, Kiro, Codex, Opencode, Gemini CLI, Goose, and the VS Code extensions Cline / Roo / Continue. Full details: [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md).

---

## Packages

This is a yarn-workspaces monorepo. Each package has its own `README.md`; all setup/reference docs live in [`docs/`](https://github.com/bgauryy/octocode-mcp/tree/main/docs), and all AI agent guidance lives in the root [`AGENTS.md`](https://github.com/bgauryy/octocode-mcp/blob/main/AGENTS.md).

| Package | Purpose |
|---------|---------|
| [`octocode-mcp`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-mcp) | MCP server: 14 tools across GitHub, local FS, LSP |
| [`octocode-cli`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-cli) | CLI: installer, tool runner, skills marketplace |
| [`octocode-vscode`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-vscode) | VS Code extension: GitHub OAuth + multi-editor MCP install |
| [`octocode-shared`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-shared) | Shared utilities: credentials, session, platform |
| [`octocode-security-utils`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-security-utils) | Standalone security utilities |

---

## Skills

> [Agent Skills](https://agentskills.io/what-are-skills) are a lightweight, open format for extending AI agent capabilities.
> Skills index: [skills/README.md](https://github.com/bgauryy/octocode-mcp/blob/main/skills/README.md)

**Research & Code Analysis**

| Skill | What it does |
|-------|--------------|
| [**Researcher**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-researcher) | Code search & exploration: local LSP + external (GitHub, npm/PyPI) |
| [**Research**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-research) | Multi-phase research with sessions, checkpoints, state persistence |
| [**Engineer**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-engineer) | Understand, write, analyze, audit code: AST + LSP + dependency graph |
| [**Brainstorming**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-brainstorming) | Idea validation grounded in evidence: GitHub, npm/PyPI, web in parallel |
| [**News**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-news) | What's new in AI, dev tools, web platform, security, notable repos |

**Planning & Writing**

| Skill | What it does |
|-------|--------------|
| [**Plan**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-plan) | Evidence-based planning: Understand > Research > Plan > Implement |
| [**RFC Generator**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-rfc-generator) | Formal technical decisions with alternatives, trade-offs, and recommendations |
| [**Doc Writer**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-documentation-writer) | 6-phase pipeline producing 16+ validated docs |
| [**Prompt Optimizer**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-prompt-optimizer) | Turn weak prompts into enforceable agent protocols |
| [**Agentic Flow**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/agentic-flow-best-practices) | Thinking framework for designing/reviewing MCP & multi-agent workflows |

**Review & Critique**

| Skill | What it does |
|-------|--------------|
| [**PR Reviewer**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-pull-request-reviewer) | PR & local code review across 7 domains with LSP flow tracing |
| [**Roast**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-roast) | Brutal code critique with file:line citations and severity levels |

**Build & Output**

| Skill | What it does |
|-------|--------------|
| [**Slides**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-slides) | Polished multi-file HTML presentations via 6-phase design flow |
| [**Design**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-design) | Dynamic DESIGN.md generator covering visual language, components, a11y |
| [**Chrome DevTools**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-chrome-devtools) | CDP-level browser debugging: network, console, perf, DOM, screenshots |

**Tooling & Setup**

| Skill | What it does |
|-------|--------------|
| [**Install**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-install) | Interactive step-by-step Octocode installer for macOS and Windows |
| [**CLI**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-cli) | Run Octocode MCP tools from the terminal without wiring MCP |
| [**Search Skill**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-search-skill) | Find, evaluate, install, refactor Agent Skills (SKILL.md format) |
| [**Stats**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-stats) | Local HTML dashboard from Octocode MCP usage stats |

https://github.com/user-attachments/assets/5b630763-2dee-4c2d-b5c1-6335396723ec

---

## Documentation

Full index: **[docs/README.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/README.md)**. All monorepo documentation lives in [`docs/`](https://github.com/bgauryy/octocode-mcp/tree/main/docs) (no per-package `docs/`).

**Docs map**
- [`docs/configuration/`](https://github.com/bgauryy/octocode-mcp/tree/main/docs/configuration): install, auth providers, MCP clients, env/config, troubleshooting
- [`docs/dev/`](https://github.com/bgauryy/octocode-mcp/tree/main/docs/dev): tool/API references, workflows, architecture, contributing, skills
- [`docs/specs/`](https://github.com/bgauryy/octocode-mcp/tree/main/docs/specs): design specs and RFCs

**Setup**
- [Authentication Setup](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md) · [GitHub](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/GITHUB_SETUP_GUIDE.md)
- [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md)
- [Using octocode-mcp with Pi](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/clients/PI_SETUP_GUIDE.md)

**Tool References**
- [GitHub Tools](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/GITHUB_TOOLS_REFERENCE.md)
- [Local + LSP Tools](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LOCAL_TOOLS_REFERENCE.md)
- [Clone & Local Workflow](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/workflows/CLONE_AND_LOCAL_TOOLS_WORKFLOW.md)

**CLI & Skills**
- [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md)
- [Skills Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/SKILLS_GUIDE.md) · [Skills Index](https://github.com/bgauryy/octocode-mcp/blob/main/skills/README.md)
- [CLI vs MCP Benchmark](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/workflows/BENCHMARK.md)

**Shared Internals**
- [Shared API Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/SHARED_API_REFERENCE.md) · [Credentials Architecture](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/architecture/CREDENTIALS_ARCHITECTURE.md) · [Session Persistence](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/architecture/SESSION_PERSISTENCE.md)

**Operations**
- [Troubleshooting](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/TROUBLESHOOTING.md) · [Development Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/DEVELOPMENT_GUIDE.md) · [Agent Guidance (AGENTS.md)](https://github.com/bgauryy/octocode-mcp/blob/main/AGENTS.md)

### The Manifest

**"Code is Truth, but Context is the Map."** Read the [Manifest for Research Driven Development](https://github.com/bgauryy/octocode-mcp/blob/main/MANIFEST.md) to understand the philosophy behind Octocode.

---

### Contributing

See the [Development Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/DEVELOPMENT_GUIDE.md) for monorepo setup, testing, and contribution guidelines.

---

<div align="center">
  <sub>Built with care for the AI Engineering Community</sub>
</div>
