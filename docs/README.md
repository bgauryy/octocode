# Octocode Documentation

All monorepo documentation lives here. No per-package `docs/` directories.

---

## MCP Server

Install, configure, and use the Octocode MCP server.

| Doc | When to read |
|-----|--------------|
| [Configuration](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CONFIGURATION.md) | All env vars and `.octocoderc` options |
| [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md) | GitHub login, token priority, Enterprise, clone tools |
| [Credentials Architecture](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CREDENTIALS.md) | Token storage, encryption, refresh chain |
| [Session Persistence](https://github.com/bgauryy/octocode/blob/main/docs/mcp/SESSION.md) | Deferred writes, exit handlers, usage statistics |

**Tools**

| Doc | When to read |
|-----|--------------|
| [GitHub Tools](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/GITHUB_TOOLS.md) | `ghSearchCode`, `ghGetFileContent`, `ghViewRepoStructure`, `ghSearchRepos`, `ghHistoryResearch`, `ghCloneRepo` |
| [Local Tools](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LOCAL_TOOLS.md) | `localSearchCode` (+ AST structural), `localViewStructure`, `localFindFiles`, `localGetFileContent`, `localBinaryInspect` |
| [Binary Tools](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/BINARY_TOOLS.md) | `localBinaryInspect` — archives, compressed streams, native binaries |
| [LSP Tools](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LSP_TOOLS.md) | `lspGetSemantics` — definitions, references, callers, callees, hover |
| [Tool Behavior Guide](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/TOOL_BEHAVIOR.md) | Known behaviors, tradeoffs, and control patterns per tool |

**Workflows**

| Doc | When to read |
|-----|--------------|
| [Agent Research Workflows](https://github.com/bgauryy/octocode/blob/main/docs/AGENT_RESEARCH_WORKFLOWS.md) | Canonical agent workflows for package, GitHub, OQL, cache/fetch/clone, local, AST, LSP, artifacts, and diff research |
| [Search Guide](https://github.com/bgauryy/octocode/blob/main/docs/context/SEARCH_GUIDE.md) | Practical ripgrep, structural AST, LSP, and exact-read best practices for code research |
| [Clone & Local Workflow](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CLONE_WORKFLOW.md) | Pull a GitHub repo, then analyze locally with LSP |
| [Tool Verification](https://github.com/bgauryy/octocode/blob/main/docs/mcp/TOOL_VERIFICATION.md) | Release-grade tool contract verification playbook |

---

## CLI

Install, run, and manage Octocode from the terminal.

| Doc | When to read |
|-----|--------------|
| [Octocode CLI Guide](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md) | User-facing CLI guide: commands, workflows, flags, tool runner, and MCP alignment |

---

## Query Language

Design notes for the unified `npx octocode search` / OQL language.

| Doc | When to read |
|-----|--------------|
| [Octocode Query Language](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_QUERY_LANGUAGE.md) | Typed query language behind `npx octocode search` |
| [OQL Research Graph Flow](https://github.com/bgauryy/octocode/blob/main/docs/context/OQL_RESEARCH_GRAPH_FLOW.md) | Research/graph proof flow and next-step continuations |

---

## Guides

| Doc | When to read |
|-----|--------------|
| [Development Guide](https://github.com/bgauryy/octocode/blob/main/docs/DEVELOPMENT_GUIDE.md) | Monorepo setup, commands, testing standards |
| [Skills Guide](https://github.com/bgauryy/octocode/blob/main/docs/SKILLS_GUIDE.md) | Install GitHub Agent Skill folders or named Octocode skills into supported agent skill directories |
| [Pi Setup Guide](https://github.com/bgauryy/octocode/blob/main/docs/PI/PI_SETUP_GUIDE.md) | Configure the Octocode Harness in Pi: skills, `npx octocode`, prompt tuning, and custom models |
| [Pi APPEND_SYSTEM starter](https://github.com/bgauryy/octocode/blob/main/docs/PI/APPEND_SYSTEM.md) | Compact system-prompt addendum for Pi + Octocode research-driven development |
