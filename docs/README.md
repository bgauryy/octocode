# Octocode documentation

Each topic has one owner doc; other docs link to it instead of repeating it. The root [README](../README.md) introduces the toolkit and its [quick start](../README.md#quick-start).

| Doc | Owns |
|---|---|
| [OCTOCODE_PROTOCOL.md](OCTOCODE_PROTOCOL.md) | The concept: evidence dimensions, the research loop, why each part exists, measured strengths and limits |
| [OCTOCODE_WORKFLOWS.md](OCTOCODE_WORKFLOWS.md) | Research-tool routing: which tool first, what each result proves, one diagram per flow |
| [OCTOCODE_TOOLS.md](OCTOCODE_TOOLS.md) | Every tool's fields, defaults, limits, results, and continuations |
| [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md) | The shared request and result envelope: rows, result shapes, `next` continuations, `related` calls, and prose `hints` |
| [OCTOCODE_CLASIFY.md](OCTOCODE_CLASIFY.md) | `classification` on search and fetch, and the beta `clasify` tool: choose a route, read the answers, limits and gates |
| [OCTOCODE_MCP.md](OCTOCODE_MCP.md) | The MCP server: client setup, registered tools, instructions, lifecycle |
| [OCTOCODE_CLI.md](OCTOCODE_CLI.md) | The CLI: commands, flags, output, exit codes |
| [CONFIGURATION.md](CONFIGURATION.md) | Config files, precedence, storage and caches, feature gates, fixed internal limits, troubleshooting |
| [generated/CONFIG_SETTINGS.md](generated/CONFIG_SETTINGS.md) | Generated table of every setting, env var, default, and range |
| [AUTHENTICATION.md](AUTHENTICATION.md) | GitHub tokens, OAuth, `gh` passthrough, Enterprise, the `clasify` key, npm credentials |
| [SECURITY.md](SECURITY.md) | Input validation, secret redaction, filesystem policy, credentials, egress |
| [SUPPORTED_LANGUAGES_AND_FEATURES.md](../packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md) | Languages, grammars, and what each feature supports |
| [LSP_SERVER_LIFECYCLE.md](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md) | Language servers: discovery, install, lifecycle |
| [CODE_GRAPH.md](../packages/octocode-native/docs/engine/CODE_GRAPH.md) | The persisted code graph: `graph ingest` and `graph query` |

Philosophy: [MANIFEST.md](../MANIFEST.md) (Research Driven Development). Releases: [CHANGELOG.md](../CHANGELOG.md). Legal: [PRIVACY.md](../PRIVACY.md) · [TERMS.md](../TERMS.md).

Hosts and packages: [VS Code](../packages/octocode-vscode/README.md) · [Pi](../packages/octocode-pi-extension/README.md) · [Claude Code](../packages/octocode-claude-plugin/README.md) · [Codex](../packages/octocode-codex-plugin/README.md) · [Agents communication](../packages/octocode-agents-communication/README.md) · [Chrome DevTools](../packages/octocode-chrome-devtools/README.md) · [Benchmark](../packages/octocode-benchmark/README.md).

Skills: [skills/](../skills/README.md) (published, including [workflow orchestration](../skills/octocode-workflows/README.md)), [skills-beta/](../skills-beta/README.md) (tested, unpublished), [skills-dev/](../skills-dev/README.md) (repository-only helpers). [OCTOCODE_WORKFLOWS.md](OCTOCODE_WORKFLOWS.md) routes research tools; the public `octocode-workflows` skill selects and executes agent workflows. Install public skills with [`npx octocode skill`](OCTOCODE_CLI.md#skill--agent-skills).

Working on this repository: [AGENTS.md](../AGENTS.md) and [DEVELOPMENT.md](../skills-dev/octocode-dev/docs/DEVELOPMENT.md), which links the dev docs and the package map.
