# Octocode: Research Driven Development for AI

<div align="center">
  <img src="https://github.com/bgauryy/octocode-mcp/raw/main/packages/octocode-mcp/assets/logo_white.png" width="400px" alt="Octocode Logo">
  
  <h3>Stop Guessing. Start Knowing.</h3>
  <p><strong>Empower your AI assistant with the skills of a Senior Staff Engineer.</strong></p>
  
  <p>
    <a href="https://octocode.ai"><strong>octocode.ai</strong></a>
  </p>
</div>

---

## Installation

> **Prerequisites**: GitHub authentication required. See [Authentication Setup](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md).

### Recommended: Octocode CLI

```bash
npx octocode-cli install
```

Interactive setup wizard with GitHub OAuth, MCP server installation, and skills marketplace. Pass `--ide <client>` for non-interactive install (for example, `npx octocode-cli install --ide cursor`), and `-m direct` only when you want to point a client at a locally installed MCP binary.

### Alternative Methods

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

---

## MCP Server

The [Octocode MCP Server](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-mcp) connects your AI assistant to code:

- **GitHub**: Search repositories, find usage patterns, read implementations, explore PRs
- **Local Tools**: Search code (ripgrep), browse directories, find files in your local codebase
- **LSP Intelligence**: Go to Definition, Find References, Call Hierarchy — compiler-level understanding
- **Package Discovery**: Resolve npm/PyPI packages to their source repos

### Benchmark Snapshot

Hermetic evals: **212/212 passing**. For agent research, Octocode MCP is the best default: it wins the combined benchmark with **99/105 quality** and **17,274 output tokens** on the full 60-query remote sweep (**89% less than raw `gh`**).

**Token benchmark — lower is better**

| Method | Token load | Tokens | Result |
|---|---:|---:|---|
| raw `gh` | `████████████████████` | 153,042 | baseline |
| Octocode CLI | `████░░░░░░░░░░░░░░░░` | 29,365 | 81% less than `gh` |
| **Octocode MCP** | `██░░░░░░░░░░░░░░░░░░` | **17,274** | **89% less than `gh`** |

**Quality benchmark — higher is better**

| Method | Quality bar | Score | Best use |
|---|---:|---:|---|
| **Octocode MCP** | `███████████████████░` | **99/105 · 94%** | Deep agent research + local/LSP flow |
| Octocode CLI | `███████████████░░░░░` | 79/105 · 75% | Short scripted research |
| raw `gh` | `not scored` | baseline | Writes and direct GitHub API access |

**Token × Quality visual axis**

X-axis = token savings vs raw `gh` (right is better). Y-axis = research quality score (up is better). **Best overall is the upper-right quadrant.**

```text
Quality ↑
100 |                                                  ● Octocode MCP
 90 |                                                    99/105 quality
 80 |                                      ● Octocode CLI 89% token savings
 70 |                                        79/105 quality
 60 |
 50 |
 40 |
 30 |
 20 |
 10 |
  0 | ● raw gh
    +--------------------------------------------------------------→ Token savings
      0%               40%               80%              90%+
      baseline                          CLI 81%        MCP 89%
```

| Point | X: token benchmark | Y: quality benchmark | Interpretation |
|---|---:|---:|---|
| **Octocode MCP** | 89% less than `gh` | **99/105** | Best combined token + quality result |
| Octocode CLI | 81% less than `gh` | 79/105 | Best short/scripted structured runner |
| raw `gh` | baseline | not scored | Direct API/writes; verbose reads |

**Best-by-scenario matrix**

| Scenario | Best tokens | Best quality | Recommendation |
|---|---|---|---|
| Full remote research sweep | **Octocode MCP** | **Octocode MCP** | Default for agent research |
| Short one-off scripted run | **Octocode CLI** | Octocode CLI / MCP | Use CLI when MCP init is not amortized |
| Shallow PR listing | **Octocode MCP** | **Octocode MCP** | MCP for triage |
| PR triage with diff stats | **Octocode MCP** | **Octocode MCP** | MCP avoids `1 + N` follow-up calls |
| Remote directory browsing | **Octocode MCP** | **Octocode MCP** | Raw `gh api /contents` is very verbose |
| Local shallow grep/find | **Octocode local tools** | **Octocode local tools** | Octocode for evidence |
| Local targeted code read | **Octocode local tools** | **Octocode local tools** | Use `matchString` / line ranges |
| Local semantic flow | **Octocode MCP LSP** | **Octocode MCP LSP** | Definitions, references, call hierarchy |
| GitHub writes | **raw `gh`** | **raw `gh`** | Octocode is read-only |

Local note: Octocode local tools win for structured evidence, metadata, targeted reads, PCRE2, and LSP (`definition`, `references`, `call hierarchy`). Octocode `verbosity:"concise"` is available for lossy broad probes; use compact/default for evidence. Details: [Benchmark Suite](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/github/README.md).

https://github.com/user-attachments/assets/de8d14c0-2ead-46ed-895e-09144c9b5071

---

## Packages

This is a yarn-workspaces monorepo. Each package has its own `README.md`; all setup/reference docs live in [`docs/`](https://github.com/bgauryy/octocode-mcp/tree/main/docs), and all AI agent guidance lives in the root [`AGENTS.md`](https://github.com/bgauryy/octocode-mcp/blob/main/AGENTS.md).

| Package | Purpose |
|---------|---------|
| [`octocode-mcp`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-mcp) | MCP server — 14 tools across GitHub, local FS, LSP |
| [`octocode-cli`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-cli) | CLI — installer, tool runner, skills marketplace |
| [`octocode-vscode`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-vscode) | VS Code extension — GitHub OAuth + multi-editor MCP install |
| [`octocode-shared`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-shared) | Shared utilities — credentials, session, platform |
| [`octocode-security-utils`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-security-utils) | Standalone security utilities |

---

## Skills

> [Agent Skills](https://agentskills.io/what-are-skills) are a lightweight, open format for extending AI agent capabilities.
> Skills index: [skills/README.md](https://github.com/bgauryy/octocode-mcp/blob/main/skills/README.md)

**Research & Code Analysis**

| Skill | What it does |
|-------|--------------|
| [**Researcher**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-researcher) | Code search & exploration — local LSP + external (GitHub, npm/PyPI) |
| [**Research**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-research) | Multi-phase research with sessions, checkpoints, state persistence |
| [**Engineer**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-engineer) | Understand, write, analyze, audit code — AST + LSP + dependency graph |
| [**Brainstorming**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-brainstorming) | Idea validation grounded in evidence — GitHub, npm/PyPI, web in parallel |
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

Full index: **[docs/README.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/README.md)**. All monorepo documentation lives in [`docs/`](https://github.com/bgauryy/octocode-mcp/tree/main/docs) — no per-package `docs/`.

**Docs map**
- [`docs/configuration/`](https://github.com/bgauryy/octocode-mcp/tree/main/docs/configuration) — install, auth providers, MCP clients, env/config, troubleshooting
- [`docs/dev/`](https://github.com/bgauryy/octocode-mcp/tree/main/docs/dev) — tool/API references, workflows, architecture, contributing, skills
- [`docs/specs/`](https://github.com/bgauryy/octocode-mcp/tree/main/docs/specs) — design specs and RFCs

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

**"Code is Truth, but Context is the Map."** -- Read the [Manifest for Research Driven Development](https://github.com/bgauryy/octocode-mcp/blob/main/MANIFEST.md) to understand the philosophy behind Octocode.

---

### Contributing

See the [Development Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/DEVELOPMENT_GUIDE.md) for monorepo setup, testing, and contribution guidelines.

---

<div align="center">
  <sub>Built with care for the AI Engineering Community</sub>
</div>
