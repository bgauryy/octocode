# MCP Server Docs

The Octocode MCP server exposes Octocode research tools to AI coding clients via the Model Context Protocol.

## Configuration

| Doc | Purpose |
|-----|---------|
| [CONFIGURATION.md](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CONFIGURATION.md) | All env vars, `.octocoderc` options, precedence, local state paths |
| [AUTHENTICATION.md](https://github.com/bgauryy/octocode/blob/main/docs/mcp/AUTHENTICATION.md) | GitHub OAuth, token priority, Enterprise, clone auth |

## Tools

| Doc | Purpose |
|-----|---------|
| [tools/GITHUB_TOOLS.md](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/GITHUB_TOOLS.md) | `ghSearchCode`, `ghGetFileContent`, `ghViewRepoStructure`, `ghSearchRepos`, `ghHistoryResearch`, `ghCloneRepo` |
| [tools/LOCAL_TOOLS.md](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LOCAL_TOOLS.md) | `localSearchCode` (+ AST/structural), `localViewStructure`, `localFindFiles`, `localGetFileContent`, `localBinaryInspect` |
| [tools/BINARY_TOOLS.md](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/BINARY_TOOLS.md) | `localBinaryInspect` — archives, compressed streams, native binaries |
| [tools/LSP_TOOLS.md](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LSP_TOOLS.md) | `lspGetSemantics` — definitions, references, callers, callees, hover |
| [tools/TOOL_BEHAVIOR.md](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/TOOL_BEHAVIOR.md) | Known behaviors, tradeoffs, and control patterns per tool |

## Workflows

      | Doc | Purpose |
      |-----|---------|
| [../context/AGENT_RESEARCH_WORKFLOWS.md](https://github.com/bgauryy/octocode/blob/main/docs/context/AGENT_RESEARCH_WORKFLOWS.md) | Product-level research flows across package search, GitHub search/fetch/clone, OQL, cache, local ripgrep, AST, LSP, artifacts, and diff |
      | [CLONE_WORKFLOW.md](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CLONE_WORKFLOW.md) | Clone a repo then analyze locally with LSP |
      | [TOOL_VERIFICATION.md](https://github.com/bgauryy/octocode/blob/main/docs/mcp/TOOL_VERIFICATION.md) | Release-grade contract and pagination verification |

## Architecture

| Doc | Purpose |
|-----|---------|
| [CREDENTIALS.md](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CREDENTIALS.md) | Token storage, AES-256-GCM encryption, refresh chain |
| [SESSION.md](https://github.com/bgauryy/octocode/blob/main/docs/mcp/SESSION.md) | Session identity, deferred writes, usage stats |

[← docs index](https://github.com/bgauryy/octocode/blob/main/docs/README.md)
