# Octocode: Research Driven Development for AI

<div align="center">
  <img src="https://github.com/bgauryy/octocode-mcp/raw/main/packages/octocode-mcp/assets/logo_white.png" width="400px" alt="Octocode Logo">
  
  <h3>Research like a Senior Staff Engineer.<br/>In every codebase, in seconds.</h3>
  <p><strong>Stop guessing.</strong> Octocode gives AI agents and humans one evidence-first research engine for local workspaces, GitHub repositories, pull requests, npm packages, and LSP semantic navigation.</p>
  <p>It has <strong>two interfaces</strong>: an <strong>MCP server</strong> for AI assistants and a <strong>CLI</strong> for terminals, scripts, and CI.</p>

  <p>
    <a href="https://octocode.ai"><strong>octocode.ai</strong></a>
    &nbsp;·&nbsp;
    <a href="#quickstart">Quickstart</a>
    &nbsp;·&nbsp;
    <a href="#two-interfaces-one-engine">Interfaces</a>
    &nbsp;·&nbsp;
    <a href="#all-12-octocode-tools">Tools</a>
    &nbsp;·&nbsp;
    <a href="#cli-commands">CLI</a>
    &nbsp;·&nbsp;
    <a href="#develop-locally">Develop</a>
    &nbsp;·&nbsp;
    <a href="#packages">Packages</a>
    &nbsp;·&nbsp;
    <a href="#skills">Skills</a>
  </p>
</div>

---

## Quickstart

Pick the path that matches where you want Octocode to show up.

### Add Octocode to an AI Assistant

```bash
# Interactive installer for Cursor, Claude Code, Windsurf, Codex, and more.
npx octocode-cli install

# Non-interactive install for a specific client.
npx octocode-cli install --ide cursor
```

Then authenticate GitHub access:

```bash
npx octocode-cli login
npx octocode-cli status
```

If you installed the CLI globally or with Homebrew, use `octocode` instead of `npx octocode-cli`.

### Use Octocode From the Terminal

```bash
# macOS / Linux
brew install bgauryy/octocode/octocode

# npm
npm install -g octocode-cli

# First useful local loop
octocode tree .
octocode search "TODO" .
octocode get README.md
```

For GitHub research, login once and then point commands at `owner/repo`:

```bash
octocode login
octocode tree facebook/react
octocode search "useState" facebook/react
octocode pr vercel/next.js#12345
```

### Common Workflows

| Goal | Start with | Then |
|------|------------|------|
| Understand a local codebase | `octocode tree .` | `octocode search "<symbol>" .` -> `octocode get <file>` -> `octocode lsp <file> --type references` |
| Research a GitHub repo | `octocode tree owner/repo` | `octocode search "<term>" owner/repo` -> `octocode get owner/repo/path/to/file.ts` |
| Inspect a pull request | `octocode pr owner/repo#123` | Read changed files with `octocode get` or search nearby code with `octocode search`. |
| Debug MCP behavior | `octocode tools` | `octocode tools <toolName> --scheme` -> `octocode tools <toolName> --queries '<json>'` |
| Share an agent setup | `octocode context` | Copy the protocol only when an agent cannot load MCP schemas directly. |

## Two Interfaces, One Engine

Octocode is not a chat prompt or a loose wrapper around `grep`. It is a tool runtime with a shared core: the MCP server and the CLI call the same tool catalog, the same security layer, the same response shaping, and the same Rust-backed hot paths.

| Interface | Best for | Install | What you get |
|-----------|----------|---------|--------------|
| **MCP server** | Claude Code, Cursor, Claude Desktop, Windsurf, Codex, and other MCP clients | `npx octocode-cli install` | 12 research tools exposed directly to your AI assistant |
| **CLI** | Terminal research, scripts, CI, quick lookups, debugging tool calls | `brew install bgauryy/octocode/octocode` or `npm install -g octocode-cli` | Friendly smart commands plus raw access to the same 12 tools |

The normal research loop is:

```text
discover shape -> search narrowly -> read exact slices -> trace semantics -> cite evidence
```

GitHub-backed tools require authentication. Run `octocode login`, or see [Authentication Setup](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md).

---

## MCP Server

The MCP server is the interface for AI assistants. It lets an agent search, read, trace, and compare code without pasting whole repositories into context.

```bash
npx octocode-cli install
```

Pass `--ide <client>` for a non-interactive install:

```bash
octocode install --ide cursor
octocode install --ide claude-code
```

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

Supported install targets include Cursor, Claude Code, Claude Desktop, Windsurf, Zed, Trae, Antigravity, Kiro, Codex, Opencode, Gemini CLI, Goose, VS Code Cline, Roo, and Continue.

https://github.com/user-attachments/assets/de8d14c0-2ead-46ed-895e-09144c9b5071

---

## CLI

The CLI is the terminal interface to the same engine. It has two layers:

1. **Smart commands** for common research flows. These route to local or GitHub tools automatically and avoid raw schemas.
2. **Raw tool runner** for exact MCP-equivalent calls with `octocode tools <name> --queries '<json>'`.

### Install

```bash
# Homebrew (macOS / Linux)
brew install bgauryy/octocode/octocode

# npm
npm install -g octocode-cli

# or run setup without a global install
npx octocode-cli install
```

The Homebrew command installs the binary as `octocode`.

```bash
octocode --version
octocode login
octocode status
```

### CLI Commands

| Command | What it does | Typical next step |
|---------|--------------|-------------------|
| `octocode get <path|github-ref>` | Fetch and minify a local or GitHub file; supports line ranges, match slices, pages, and `--mode none|standard|symbols`. | Use after `tree`, `files`, or `search` identifies a file. |
| `octocode tree <path|owner/repo>` | View local or GitHub directory structure. | Follow with `files` or `search`. |
| `octocode files <query> [path|owner/repo]` | Find paths or content matches locally or on GitHub; `--search path|content|both`. | Use `get` for exact evidence. |
| `octocode search <pattern> <path|owner/repo>` | Search code locally or on GitHub. | Use `get --match-string` or `lsp` from returned anchors. |
| `octocode pr <owner/repo[#N]|PR-URL>` | Search PRs or inspect one PR, including changed files, comments, commits, and selected patches. | Use `get` or `search` on files touched by the PR. |
| `octocode repo <keywords...>` | Discover repositories by keyword, topic, owner, language, stars, dates, and quality filters. | Use `tree`, `search`, or `pr` on selected repos. |
| `octocode pkg <package>` | Resolve npm metadata and source repositories. | Inspect the source repo with GitHub tools. |
| `octocode symbols <file|path>` | Produce semantic outlines for files or directories. | Use before `lsp` to choose symbol anchors. |
| `octocode lsp <file> --type <type>` | Run semantic navigation: definitions, references, callers, callees, call hierarchy, hover, document symbols, type definitions, implementations. | Use after `symbols` or `search` gives a symbol and line. |
| `octocode tools` | List, inspect, or run the raw 12 MCP tools. | Use `octocode tools <name> --scheme` before raw calls. |
| `octocode context` | Print agent protocol, routing guidance, tool list, and schemas. | Use `--full` only when inline JSON schemas are needed. |
| `octocode install` | Configure `octocode-mcp` for an IDE or MCP client. | Run `status` to verify. |
| `octocode auth` / `login` / `logout` | Manage GitHub authentication. | Use `token --source` to inspect resolution. |
| `octocode token` | Print or validate the resolved token source. | Add `--reveal` only when you intentionally need the token. |
| `octocode status` | Show auth and MCP-client health; `--sync` adds cross-client sync analysis. | Fix missing auth or client config. |
| `octocode skills` | Search, read, install, remove, list, and sync Agent Skills. | Install research/review skills into supported agents. |

Full command reference: [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md).

### Raw Tool Calls

```bash
octocode tools
octocode tools localSearchCode --scheme
octocode tools localSearchCode --queries '{"path":".","pattern":"TODO"}'
octocode tools ghSearchCode --queries '{"keywordsToSearch":["useState"],"owner":"facebook","repo":"react"}'
```

Direct CLI tool runs auto-fill `id`, `mainResearchGoal`, `researchGoal`, and `reasoning` when omitted. MCP clients should provide those research fields explicitly.

---

## All 12 Octocode Tools

| Tool | Interface | What it is for |
|------|-----------|----------------|
| `ghSearchCode` | MCP + CLI | Search code or paths across GitHub with owner, repo, path, filename, extension, and match filters. |
| `ghGetFileContent` | MCP + CLI | Read exact GitHub file content by full file, line range, match slice, char page, or signature skeleton. |
| `ghViewRepoStructure` | MCP + CLI | Browse a GitHub repository tree before reading files. |
| `ghSearchRepos` | MCP + CLI | Discover repositories by keywords, owner, topics, language, stars, size, dates, and archive state. |
| `ghSearchPRs` | MCP + CLI | Search PR history or inspect one PR's metadata, changed files, patches, comments, reviews, and commits. |
| `ghCloneRepo` | MCP + CLI | Clone a repository or sparse subtree into Octocode's local cache for local/LSP analysis. |
| `npmSearch` | MCP + CLI | Resolve npm packages to metadata and source repositories. |
| `localSearchCode` | MCP + CLI | Search local file contents with ripgrep-style filtering, pagination, snippets, and count modes. |
| `localViewStructure` | MCP + CLI | Browse local directories with depth, filters, pagination, and metadata. |
| `localFindFiles` | MCP + CLI | Find local files or directories by name, path, regex, extension, size, time, permissions, and type. |
| `localGetFileContent` | MCP + CLI | Read targeted local file content by exact slice, match, symbols, line range, or char page. |
| `lspGetSemantics` | MCP + CLI | Ask local language servers for definitions, references, callers, callees, call hierarchy, hover, document symbols, type definitions, and implementations. |

Tool behavior references:

- [GitHub Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/GITHUB_TOOLS_REFERENCE.md)
- [Local Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LOCAL_TOOLS_REFERENCE.md)
- [LSP Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LSP_TOOLS_REFERENCE.md)
- [Tool Behavior Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/TOOL_BEHAVIOR_GUIDE.md)

---

## Security, Token Efficiency, and Rust

Octocode is built for AI-agent workflows where the expensive part is not just execution time - it is irrelevant context, secret leakage, and untrusted inputs.

**Security mechanisms**

- Inputs pass through schema validation and security wrappers before execution.
- Secrets are detected and redacted in tool inputs, outputs, errors, logs, and returned content.
- Local paths are canonicalized, checked against workspace/allowed roots, and rejected when they escape allowed directories or hit ignored paths.
- Local command execution is allowlisted. Tools use controlled builders for commands such as `rg`, `find`, `ls`, and `git`; arguments are not passed through a free-form shell.
- GitHub token resolution is explicit: `OCTOCODE_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`, encrypted Octocode credentials, then `gh auth token`.
- Clone-backed workflows require local/clone enablement and materialize into managed cache locations.

**Token efficiency**

- `standard` minification removes comments and blank-line noise while preserving readable code shape.
- `symbols` mode returns structural outlines with line numbers so agents can map a file before reading bodies.
- Match-based and line-based reads keep the model on the exact evidence instead of whole files.
- Bulk tools paginate results and large payloads; agents can continue from `page`, `charOffset`, or response pagination fields.
- Tool responses prefer compact, structured YAML by default because it is easier for agents to scan than raw JSON.

**Rust-backed hot paths**

Octocode uses Rust where it changes the feel of the product, not as a vanity rewrite:

- `octocode-security` runs high-volume secret detection and masking through Rust's linear-time regex engine.
- `@octocodeai/octocode-context-utils` handles agent-readable minification, semantic signatures, UTF-8/UTF-16 offsets, ripgrep JSON parsing, diff filtering, and YAML serialization.
- `octocode-lsp` owns native LSP runtime pieces: language detection, server command resolution, stdio JSON-RPC, symbol anchoring, pooled clients, and semantic requests.

That combination keeps flows fast and predictable: search broadly, read narrowly, trace semantically, return compact evidence.

---

## Develop Locally

Run these from the repository root unless a package doc says otherwise.

```bash
yarn install
yarn build
yarn test:quiet
yarn lint
```

| Task | Command |
|------|---------|
| Install dependencies | `yarn install` |
| Build every package | `yarn build` |
| Run the quieter test lane | `yarn test:quiet` |
| Run full coverage | `yarn test` |
| Lint all packages | `yarn lint` |
| Fix lint/format issues where possible | `yarn lint:fix` |
| Validate MCP package contracts | `yarn mcp:contracts` |
| Run the MCP package gate | `yarn mcp:package` |
| Validate CLI registries | `cd packages/octocode-cli && yarn validate:mcp && yarn validate:skills` |

Useful editing rules for this repo:

- Documentation links in `docs/` and package READMEs use absolute GitHub URLs.
- MCP behavior changes usually need tests under `packages/octocode-mcp/tests/` or the owning package's `tests/` directory.
- Tool descriptions and schemas come from the shared tool catalog, so update the shared source instead of patching generated output.
- Generated folders such as `dist/`, `out/`, `coverage/`, and `node_modules/` are not source.

For the full workflow, see the [Development Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/DEVELOPMENT_GUIDE.md).

### Troubleshooting Fast

| Symptom | Try |
|---------|-----|
| GitHub queries fail or return less than expected | Run `octocode login`, then `octocode status` to confirm the token source. |
| An MCP client does not show Octocode tools | Run `octocode status --sync`, then restart the client so it reloads MCP config. |
| Local tools cannot see the files you expect | Check `WORKSPACE_ROOT` and `ALLOWED_PATHS` in the [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md). |
| Output is too large | Use `octocode search` first, then `octocode get --match-string` or a line range instead of reading whole files. |
| LSP results are hard to target | Run `octocode symbols <file>` before `octocode lsp <file> --type <type>`. |

---

## Packages

This is a yarn-workspaces monorepo. Runtime code is split so the MCP server, CLI, and extension share one tool core instead of each reimplementing research behavior. Setup/reference docs live in [`docs/`](https://github.com/bgauryy/octocode-mcp/tree/main/docs), and AI-agent guidance lives in [`AGENTS.md`](https://github.com/bgauryy/octocode-mcp/blob/main/AGENTS.md).

| Directory | npm package | Purpose |
|-----------|-------------|---------|
| [`packages/octocode-mcp`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-mcp) | `octocode-mcp` | MCP server that registers the 12 Octocode tools for AI assistants. |
| [`packages/octocode-cli`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-cli) | `octocode-cli` | Terminal interface: smart research commands, raw tool runner, auth, install, status, token, and skills workflows. |
| [`packages/octocode-tools-core`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-tools-core) | `@octocodeai/octocode-tools-core` | Shared tool catalog and implementations for GitHub, local filesystem, package search, and LSP flows. |
| [`packages/octocode-context-utils`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-context-utils) | `@octocodeai/octocode-context-utils` | Rust-backed context engine for minification, signatures, pagination offsets, ripgrep parsing, diff filtering, and YAML output. |
| [`packages/octocode-security`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-security) | `octocode-security` | Rust-backed secret detection plus TypeScript path, command, input, and tool security utilities. |
| [`packages/octocode-lsp`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-lsp) | `octocode-lsp` | Rust-native LSP runtime for language detection, server config, JSON-RPC, symbol anchoring, pooled clients, and semantic navigation. |
| [`packages/octocode-shared`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-shared) | `octocode-shared` | Shared credentials, token resolution, session persistence, and platform utilities. |
| [`packages/octocode-vscode`](https://github.com/bgauryy/octocode-mcp/tree/main/packages/octocode-vscode) | `octocode-mcp-vscode` | VS Code extension for GitHub OAuth and multi-editor MCP installation. |

---

## Skills

> [Agent Skills](https://agentskills.io/what-are-skills) are a lightweight, open format for extending AI agent capabilities.
> Skills index: [skills/README.md](https://github.com/bgauryy/octocode-mcp/blob/main/skills/README.md)

**Research & Code Analysis**

| Skill | What it does |
|-------|--------------|
| [**Researcher**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-researcher) | Code search & exploration: local LSP + external (GitHub, npm) |
| [**Research**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-research) | Multi-phase research with sessions, checkpoints, state persistence |
| [**Engineer**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-engineer) | Understand, write, analyze, audit code: AST + LSP + dependency graph |
| [**Brainstorming**](https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-brainstorming) | Idea validation grounded in evidence: GitHub, npm, web in parallel |
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
- [Authentication Setup](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md)
- [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md)
- [Using octocode-mcp with Pi](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/clients/PI_SETUP_GUIDE.md)

**Tool References**
- [GitHub Tools](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/GITHUB_TOOLS_REFERENCE.md)
- [Local Tools](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LOCAL_TOOLS_REFERENCE.md)
- [LSP Tools](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LSP_TOOLS_REFERENCE.md)
- [Clone & Local Workflow](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/workflows/CLONE_AND_LOCAL_TOOLS_WORKFLOW.md)

**CLI & Skills**
- [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md)
- [Skills Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/SKILLS_GUIDE.md) · [Skills Index](https://github.com/bgauryy/octocode-mcp/blob/main/skills/README.md)

**Shared Internals**
- [Credentials Architecture](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/architecture/CREDENTIALS_ARCHITECTURE.md) · [Session Persistence](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/architecture/SESSION_PERSISTENCE.md)

**Operations**
- [Development Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/DEVELOPMENT_GUIDE.md) · [Agent Guidance (AGENTS.md)](https://github.com/bgauryy/octocode-mcp/blob/main/AGENTS.md)

### The Manifest

**"Code is Truth, but Context is the Map."** Read the [Manifest for Research Driven Development](https://github.com/bgauryy/octocode-mcp/blob/main/MANIFEST.md) to understand the philosophy behind Octocode.

---

### Contributing

See the [Development Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/DEVELOPMENT_GUIDE.md) for monorepo setup, testing, and contribution guidelines.

---

<div align="center">
  <sub>Built with care for the AI Engineering Community</sub>
</div>
