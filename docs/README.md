# Octocode Documentation

All monorepo documentation lives here. No per-package `docs/` directories.

---

## Configuration

Install, authenticate, and configure Octocode.

| Doc | When to read |
|-----|--------------|
| [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md) | All env vars and `.octocoderc` options |

**Providers**

| Doc | When to read |
|-----|--------------|
| [Authentication Setup](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md) | GitHub login, token priority, Enterprise, clone tools |

**Clients**

| Doc | When to read |
|-----|--------------|
| [Pi Setup Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/clients/PI_SETUP_GUIDE.md) | Use Octocode inside earendil-works/pi via `pi-mcp-adapter` |

> For all other supported MCP clients, use `npx octocode-cli install --ide <client>` — see the [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md#install) for the full client list.

---

## Development

How to use, extend, and contribute to Octocode.

| Doc | When to read |
|-----|--------------|
| [Development Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/DEVELOPMENT_GUIDE.md) | Monorepo setup, commands, testing standards |
| [Skills Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/SKILLS_GUIDE.md) | Install, build, and browse the skills marketplace |

**Tool & API References**

| Doc | When to read |
|-----|--------------|
| [GitHub Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/GITHUB_TOOLS_REFERENCE.md) | Schemas + behavior for remote-host tools |
| [Local Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LOCAL_TOOLS_REFERENCE.md) | Schemas + behavior for local filesystem tools |
| [LSP Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LSP_TOOLS_REFERENCE.md) | Canonical reference for semantic navigation |
| [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md) | All `octocode-cli` commands and flags |

**Workflows**

| Doc | When to read |
|-----|--------------|
| [Clone & Local Tools Workflow](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/workflows/CLONE_AND_LOCAL_TOOLS_WORKFLOW.md) | Pull a GitHub repo, then analyze locally with LSP |
| [Advanced MCP Tool Verification](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/workflows/ADVANCED_MCP_TOOL_VERIFICATION.md) | Release-grade tool contract verification playbook |
| [CLI vs MCP Benchmark](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/workflows/BENCHMARK.md) | Agent benchmark harness comparing MCP and CLI paths |

**Architecture**

| Doc | When to read |
|-----|--------------|
| [Credentials Architecture](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/architecture/CREDENTIALS_ARCHITECTURE.md) | Token storage, encryption, refresh chain |
| [Session Persistence](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/architecture/SESSION_PERSISTENCE.md) | Deferred writes, exit handlers, usage statistics |
