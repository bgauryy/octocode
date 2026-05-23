# Documentation Index

Two top-level sections — pick by what you're doing:

| | What's here |
|---|---|
| **[configuration/](https://github.com/bgauryy/octocode-mcp/tree/main/docs/configuration)** | Provider auth, MCP client setup, env vars, troubleshooting — everything to install and run Octocode |
| **[dev/](https://github.com/bgauryy/octocode-mcp/tree/main/docs/dev)** | Tool references, package internals, architecture, contribution guide — everything to develop with or on Octocode |
| **[specs/](https://github.com/bgauryy/octocode-mcp/tree/main/docs/specs)** | Design specs and RFCs |

---

## I want to…

### Install or configure Octocode → [`configuration/`](https://github.com/bgauryy/octocode-mcp/tree/main/docs/configuration)

| Need | Doc |
|------|-----|
| Pick a provider and authenticate | [Authentication Setup](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md) |
| Set up GitHub / GitLab / Bitbucket | [GitHub](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/GITHUB_SETUP_GUIDE.md) · [GitLab](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/GITLAB_SETUP_GUIDE.md) · [Bitbucket](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/BITBUCKET_SETUP_GUIDE.md) |
| Use Octocode inside Pi (earendil-works) | [Pi Setup Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/clients/PI_SETUP_GUIDE.md) |
| Tune env vars and config flags | [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md) |
| Diagnose a problem | [Troubleshooting](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/TROUBLESHOOTING.md) |

### Use or build Octocode → [`dev/`](https://github.com/bgauryy/octocode-mcp/tree/main/docs/dev)

**Tool references** — [`dev/reference/`](https://github.com/bgauryy/octocode-mcp/tree/main/docs/dev/reference)

| Need | Doc |
|------|-----|
| GitHub / GitLab / Bitbucket tools | [GitHub, GitLab & Bitbucket Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/GITHUB_GITLAB_TOOLS_REFERENCE.md) |
| Local + LSP tools | [Local & LSP Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LOCAL_TOOLS_REFERENCE.md) |
| CLI commands and tool syntax | [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md) |
| `octocode-shared` exports | [Shared API Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/SHARED_API_REFERENCE.md) |

**Workflows** — [`dev/workflows/`](https://github.com/bgauryy/octocode-mcp/tree/main/docs/dev/workflows)

| Need | Doc |
|------|-----|
| Clone a repo → run local + LSP tools on it | [Clone & Local Tools Workflow](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/workflows/CLONE_AND_LOCAL_TOOLS_WORKFLOW.md) |
| Compare CLI vs MCP throughput | [CLI vs MCP Benchmark](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/workflows/BENCHMARK.md) |

**Architecture** — [`dev/architecture/`](https://github.com/bgauryy/octocode-mcp/tree/main/docs/dev/architecture)

| Need | Doc |
|------|-----|
| Credential storage, encryption, refresh | [Credentials Architecture](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/architecture/CREDENTIALS_ARCHITECTURE.md) |
| Session lifecycle and deferred writes | [Session Persistence](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/architecture/SESSION_PERSISTENCE.md) |

**Contribute / extend** — top-level in [`dev/`](https://github.com/bgauryy/octocode-mcp/tree/main/docs/dev)

| Need | Doc |
|------|-----|
| Work on the monorepo | [Development Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/DEVELOPMENT_GUIDE.md) |
| Build or install a skill | [Skills Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/SKILLS_GUIDE.md) · [Skills Index](https://github.com/bgauryy/octocode-mcp/blob/main/skills/README.md) |

## Documentation Rules

- **All docs live here** — single source under `docs/`, organized into `configuration/` and `dev/`. No per-package `docs/`.
- **One `AGENTS.md`** — all AI agent guidance (root + per-package) lives in the root [`AGENTS.md`](https://github.com/bgauryy/octocode-mcp/blob/main/AGENTS.md).
- Use absolute GitHub URLs in documentation files.
- Keep task instructions in one canonical doc instead of repeating them in multiple references.
