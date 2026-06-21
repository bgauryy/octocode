# Octocode Documentation

All monorepo documentation lives here. No per-package `docs/` directories.

---

## MCP Server

Install, configure, and use the Octocode MCP server.

| Doc | When to read |
|-----|--------------|
| [Configuration](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CONFIGURATION.md) | All env vars and `.octocoderc` options |
| [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/mcp/AUTHENTICATION.md) | GitHub login, token priority, Enterprise, clone tools |
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
| [Agent Research Workflows](https://github.com/bgauryy/octocode/blob/main/docs/AGENT_RESEARCH_WORKFLOWS.md) | Product-level workflows for package, GitHub, OQL, cache/fetch/clone, local ripgrep, AST, LSP, artifacts, and diff research |
      | [Clone & Local Workflow](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CLONE_WORKFLOW.md) | Pull a GitHub repo, then analyze locally with LSP |
      | [Tool Verification](https://github.com/bgauryy/octocode/blob/main/docs/mcp/TOOL_VERIFICATION.md) | Release-grade tool contract verification playbook |

---

## CLI

Install, run, and manage Octocode from the terminal.

| Doc | When to read |
|-----|--------------|
| [CLI Reference](https://github.com/bgauryy/octocode/blob/main/docs/cli/REFERENCE.md) | All `octocode` commands, flags, and tool runner |
| [CLI vs MCP Benchmark](https://github.com/bgauryy/octocode/blob/main/docs/cli/BENCHMARK.md) | Agent benchmark comparing MCP and CLI paths |

---

## Query Language

Design notes for the current and future `octocode grep` / `octocode search`
language.

| Doc | When to read |
|-----|--------------|
| [Query Language Docs](https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/README.md) | Folder map, decision summary, and implementation checklist |
| [Octocode Query Language](https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OCTOCODE_QUERY_LANGUAGE.md) | North-star typed query language for local grep plus GitHub/npm/external search |
| [Octocode Query Language Plan](https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OCTOCODE_QUERY_LANGUAGE_PLAN.md) | Implementation plan, prerequisites, package split, milestones, tests, and risks |

---

## Guides

| Doc | When to read |
|-----|--------------|
| [Security Model](https://github.com/bgauryy/octocode/blob/main/docs/SECURITY.md) | Secret redaction (in + out), sanitization pipeline, path/command safety, credentials |
| [Development Guide](https://github.com/bgauryy/octocode/blob/main/docs/DEVELOPMENT_GUIDE.md) | Monorepo setup, commands, testing standards |
| [Skills Guide](https://github.com/bgauryy/octocode/blob/main/docs/SKILLS_GUIDE.md) | Install, build, and browse the skills marketplace |
| [Pi Setup Guide](https://github.com/bgauryy/octocode/blob/main/docs/PI/PI_SETUP_GUIDE.md) | Use Octocode inside earendil-works/pi via `pi-mcp-adapter` |
