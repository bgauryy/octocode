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
| **Install** | `npx octocode-cli install` | `brew tap bgauryy/octocode` → `brew install octocode` |
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

Pass `--ide <client>` for a non-interactive install (e.g. `octocode install --ide cursor`).

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

The same 14 tools, straight from your terminal — no MCP server, no IDE required.

### Install

```bash
# Homebrew (macOS / Linux) — recommended
brew install bgauryy/octocode/octocode

# or tap once, then use the short name
brew tap bgauryy/octocode
brew install octocode

# npm global
npm install -g octocode-cli

# or run once, no install
npx octocode-cli install
```

> Node is pulled in automatically by Homebrew. The installed command is **`octocode`** (not `octocode-cli`).

**Verify and sign in:**

```bash
octocode --version   # → octocode v1.5.3
octocode login       # GitHub OAuth — required for the GitHub-backed tools
```

### What you get

The CLI is two things in one binary:

1. **Setup wizard** — install the MCP server + skills, manage GitHub OAuth, sync configs across 15 editors
2. **Standalone tool runner** — call any of the 14 tools, pipe JSON to other tools, use in CI

### Commands

| Command | What it does |
|---------|--------------|
| `octocode login` / `logout` | GitHub OAuth device flow (`--hostname` for Enterprise) |
| `octocode install --ide <client>` | Configure `octocode-mcp` for an IDE/agent |
| `octocode status` | Full health check: auth + installed MCPs + cache |
| `octocode token` | Print the resolved GitHub token (`--source`, `--validate`) |
| `octocode skills` | Search / install / remove / sync Agent Skills |
| `octocode mcp` | MCP marketplace: `list` / `install` / `remove` / `status` |
| `octocode sync` | Sync MCP configs across all IDE clients (`--dry-run`, `--status`) |
| `octocode cache` | Inspect / clean repos, skills, logs, and tool caches |
| `octocode tools` | List, inspect schema, or run a tool with `--queries '<json>'` |
| `octocode instructions` | Print full MCP instructions + every tool schema |

Top-level flags: `--version`/`-v`, `--help`/`-h`, `--json`/`-j`, `--agent` (agent bootstrap).

### The 14 tools

Run any directly with `octocode tools <name> --queries '<json>'`:

| Group | Tools |
|-------|-------|
| GitHub | `githubSearchCode` · `githubSearchRepositories` · `githubSearchPullRequests` · `githubGetFileContent` · `githubViewRepoStructure` · `githubCloneRepo` |
| Local | `localSearchCode` (ripgrep) · `localFindFiles` · `localGetFileContent` · `localViewStructure` |
| LSP | `lspGotoDefinition` · `lspFindReferences` · `lspCallHierarchy` |
| Package | `packageSearch` (npm / PyPI → source repo) |

```bash
octocode tools                                                              # list all
octocode tools localSearchCode                                              # inspect schema
octocode tools localSearchCode --queries '{"path":".","pattern":"TODO"}'   # run
octocode tools githubSearchCode --queries '{"keywordsToSearch":["useState"],"owner":"facebook","repo":"react"}'
```

Supported install targets: Cursor, Claude Code, Claude Desktop, Windsurf, Zed, Trae, Antigravity, Kiro, Codex, Opencode, Gemini CLI, Goose, VS Code Cline / Roo / Continue. Full reference: [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md).

---

## Benchmark

> How does Octocode compare to using the `gh` CLI directly for GitHub research?

We ran a blind 17-question benchmark across five capability categories (code search, file content, repo structure, PR intelligence, repository search). Each agent answered independently using only its own tools; a separate judge scored every answer 0–3 and measured character usage.

```
┌─────────────────────────────────────────────────────────────────────────┐
│            GITHUB RESEARCH BENCHMARK · FINAL SCORECARD                  │
│                   17 questions · 5 categories · Jun 2026                │
├──────────────────────────┬──────────────────┬───────────────────────────┤
│ Metric                   │   octocode       │        gh CLI             │
├──────────────────────────┼──────────────────┼───────────────────────────┤
│ Quality score (non-drift)│   36 / 42  ████  │   32 / 42  ████           │
│ Quality score (all 17)   │   43 / 51  ████  │   38 / 51  ████           │
│ API calls                │     108    ██    │     407    ████████████   │
│ Total chars used         │    1.08M   █     │   13.20M   ████████████   │
│ Token-score wins         │      11    ████  │       2    █              │
│ Quality per 1k chars     │   0.0381         │   0.0027                  │
│ Efficiency advantage     │  14.2×  BETTER   │  baseline                 │
├──────────────────────────┼──────────────────┼───────────────────────────┤
│ Category wins            │  SEARCH ✅       │  CONTENT (Q7 blob read)   │
│                          │  STRUCTURE ✅    │                           │
│                          │  PR ✅           │                           │
│                          │  REPOS (tie) ≈   │  REPOS (tie) ≈            │
├──────────────────────────┼──────────────────┼───────────────────────────┤
│ OVERALL WINNER           │  ✅  octocode    │                           │
└──────────────────────────┴──────────────────┴───────────────────────────┘
```

Full benchmark — questions, raw answers, and judge scoring — lives in [`benchmark/github/`](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/github).

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
