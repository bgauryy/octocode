# Octocode user guides

Run Octocode from a terminal or connect it to your coding agent over the Model Context Protocol (MCP). Both interfaces use the same research tools. [Install Octocode and try a local read](../README.md#install)

| I want to… | Open |
|---|---|
| Connect my agent, check which tools it can use, or troubleshoot startup | [MCP setup](OCTOCODE_MCP.md) |
| Run a tool, inspect its fields, or use the result in a script | [CLI guide](OCTOCODE_CLI.md) |
| Sign in to GitHub, understand which token wins, or use GitHub Enterprise | [Authentication](AUTHENTICATION.md) |
| Change a default, set a key, or enable an optional tool | [Configuration and defaults](CONFIGURATION.md) |
| Choose among local, GitHub, package and classification tools | [Tool reference](OCTOCODE_TOOLS.md) |
| See how an agent goes from a question to source evidence | [Research workflows](OCTOCODE_PROTOCOL.md) |
| Select relevant results or ask questions about unread files | [Classification with `clasify`](OCTOCODE_CLASIFY.md) |
| Follow a cursor, interpret a partial answer, or join result pages | [Tool data and continuations](TOOL_DATA_CONTRACT.md) |
| Find cached data, change storage mode, or clear saved results | [Storage and limits](STORAGE_AND_LIMITS.md) |
| Check which files Octocode can read and what leaves my machine | [Security](SECURITY.md) and [privacy](../PRIVACY.md) |

## Language support and integrations

- [Supported languages](../packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md), [language-server setup](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md), [code graphs](../packages/octocode-native/docs/engine/CODE_GRAPH.md) and [how locations map to source bytes](../packages/octocode-native/docs/engine/SOURCE_CORRESPONDENCE.md).
- [VS Code extension](../packages/octocode-vscode/README.md), [Claude Code plugin](../packages/octocode-claude-plugin/README.md) and [Codex plugin](../packages/octocode-codex-plugin/README.md).
- [Agent skills](../skills/README.md), [tool routing](../skills/octocode-research/references/tool-flow.md), [browser tools](../packages/octocode-chrome-devtools/README.md) and [agent communication](../packages/octocode-agents-communication/README.md).

[Benchmarks](../packages/octocode-benchmark/README.md) · [Project philosophy](../MANIFEST.md) · [Development](../skills-dev/octocode-dev/docs/DEVELOPMENT.md) · [Terms](../TERMS.md)
