# Octocode documentation

Each topic has one owner doc; other docs link to it instead of repeating it. The root [README](../README.md) introduces the toolkit and its [quick start](../README.md#quick-start).

| Doc | Owns |
|---|---|
| [OCTOCODE_PROTOCOL.md](OCTOCODE_PROTOCOL.md) | The concept: evidence dimensions, the research loop, why each part exists, measured strengths and limits |
| [OCTOCODE_WORKFLOWS.md](OCTOCODE_WORKFLOWS.md) | Research routing: which tool first, what each result proves, one diagram per flow |
| [OCTOCODE_TOOLS.md](OCTOCODE_TOOLS.md) | Every tool's fields, defaults, limits, results, and continuations |
| [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md) | The shared request and result envelope: rows, result shapes, `next.*` pages, `hints.*` leads, handoffs |
| [OCTOCODE_CLASIFY.md](OCTOCODE_CLASIFY.md) | `clasify`: when to use it, worked examples, reading scores, benchmarks, limits, availability |
| [OCTOCODE_MCP.md](OCTOCODE_MCP.md) | The MCP server: client setup, registered tools, instructions, lifecycle |
| [OCTOCODE_CLI.md](../packages/octocode/docs/OCTOCODE_CLI.md) | The CLI: commands, flags, output, exit codes |
| [CONFIGURATION.md](CONFIGURATION.md) | Config files, precedence, storage and caches, feature gates, troubleshooting |
| [generated/CONFIG_SETTINGS.md](generated/CONFIG_SETTINGS.md) | Generated table of every setting, env var, default, and range |
| [AUTHENTICATION.md](AUTHENTICATION.md) | GitHub tokens, OAuth, `gh` passthrough, Enterprise, the `clasify` key, npm credentials |
| [SECURITY.md](SECURITY.md) | Input validation, secret redaction, filesystem policy, credentials, egress |
| [SUPPORTED_LANGUAGES_AND_FEATURES.md](../packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md) | Languages, grammars, and what each feature supports |
| [LSP_SERVER_LIFECYCLE.md](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md) | Language servers: discovery, install, lifecycle |
| [CODE_GRAPH.md](../packages/octocode-native/docs/engine/CODE_GRAPH.md) | The persisted code graph: `graph ingest` and `graph query` |

Philosophy: [MANIFEST.md](../MANIFEST.md) (Research Driven Development). Releases: [CHANGELOG.md](../CHANGELOG.md). Legal: [PRIVACY.md](../PRIVACY.md) · [TERMS.md](../TERMS.md).

Hosts and packages: [VS Code](../packages/octocode-vscode/README.md) · [Pi](../packages/octocode-pi-extension/README.md) · [Claude Code](../packages/octocode-claude-plugin/README.md) · [Codex](../packages/octocode-codex-plugin/README.md) · [Agents communication](../packages/octocode-agents-communication/README.md) · [Chrome DevTools](../packages/octocode-chrome-devtools/README.md) · [Benchmark](../packages/octocode-benchmark/README.md).

Skills: [skills/](../skills/README.md) (published), [skills-beta/](../skills-beta/README.md) (tested, unpublished), [skills-dev/](../skills-dev/README.md) (for this repository). Install them with [`npx octocode skill`](../packages/octocode/docs/OCTOCODE_CLI.md#skill--agent-skills).

Working on this repository: [AGENTS.md](../AGENTS.md) and [DEVELOPMENT.md](../skills-dev/octocode-dev/docs/DEVELOPMENT.md), which links the dev docs and the package map.
