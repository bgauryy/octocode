# Octocode documentation

Octocode is an agentic toolkit for evidence-driven software engineering. The toolkit combines code research, Agent Skills, CLI and MCP interfaces, native runtime primitives, host integrations, multi-agent coordination, and evaluation infrastructure.

Use this page to find the document that owns each topic. The root [README](../README.md) introduces the toolkit and its packages.

## Get started

| Goal | Guide |
|------|-------|
| Run Octocode from a terminal | [Octocode CLI guide](../packages/octocode/docs/OCTOCODE_CLI.md) |
| Connect an AI client over MCP | [Octocode MCP server](OCTOCODE_MCP.md) |
| Configure authentication, storage, tools, and providers | [Configuration and authentication](CONFIGURATION.md) |
| Understand the security boundary | [Security](SECURITY.md) |

## Research and tool reference

| Topic | Document | Type |
|-------|----------|------|
| Every public tool, field, result, and continuation | [Octocode tools reference](OCTOCODE_TOOLS.md) | Reference |
| Choosing among local text, AST, topology, file, and LSP evidence | [Local code research workflow](LOCAL_RESEARCH_WORKFLOW.md) | How-to |
| Carrying evidence and continuations between tools | [Tool data and handoff contract](TOOL_DATA_CONTRACT.md) | Reference |
| Evidence grades and agent routing rules | [Octocode research manifest](OCTOCODE_RESEARCH_MANIFEST.md) | Explanation |
| Semantic assessment contract and examples | [Semantic assessment reference](OCTOCODE_CLASIFY.md) | Reference |
| Scout, assess, then verify source evidence | [Semantic assessment research guide](CLASIFY_RESEARCH_GUIDE.md) | How-to |
| Historical Jev provider measurements | [Jev benchmark](JEV_BENCHMARK.md) | Explanation |
| The broader research-driven development philosophy | [Research-driven development manifest](../MANIFEST.md) | Explanation |

## Contributor and quality guides

| Topic | Document |
|-------|----------|
| Acceptance criteria for public tool quality | [Tool quality and agent workflow acceptance](MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md) |
| Repository-wide contributor rules and package map | [AGENTS.md](../AGENTS.md) |
| Development and release scripts | [Scripts reference](../scripts/README.md) |
| Owner-triggered release checklist and gates | [Release checklist](RELEASE.md) |

## Package guides

The monorepo contains 10 primary workspace packages. The [package overview](PACKAGES.md) explains the runtime flow and each package's role; read the package README for its public purpose and its architecture page for ownership, dependencies, and invariants.

| Package | Purpose | Guides |
|---------|---------|--------|
| `octocode` | Agent-oriented CLI and toolkit entry point | [README](../packages/octocode/README.md) · [Architecture](../packages/octocode/ARCHITECTURE.md) · [CLI guide](../packages/octocode/docs/OCTOCODE_CLI.md) |
| `octocode-mcp` | Thin stdio MCP interface | [README](../packages/octocode-mcp/README.md) · [Architecture](../packages/octocode-mcp/ARCHITECTURE.md) |
| `octocode-mcp-vscode` | VS Code OAuth and multi-editor MCP setup | [README](../packages/octocode-vscode/README.md) · [Architecture](../packages/octocode-vscode/ARCHITECTURE.md) |
| `@octocodeai/pi-extension` | Pi host integration and Pi-owned contracts | [README](../packages/octocode-pi-extension/README.md) · [Architecture](../packages/octocode-pi-extension/ARCHITECTURE.md) · [Docs index](../packages/octocode-pi-extension/docs/README.md) |
| `@octocodeai/octocode-native` | Consolidated CLI/runtime/engine distribution with separate Rust crates | [README](../packages/octocode-native/README.md) · [Architecture](../packages/octocode-native/ARCHITECTURE.md) · [LSP lifecycle](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md) |
| `@octocodeai/octocode-extension-rust` | Native workspace snapshots, mutations, history, and diffs | [README](../packages/octocode-extension-rust/README.md) · [Architecture](../packages/octocode-extension-rust/ARCHITECTURE.md) |
| `@octocodeai/config` | Shared environment and configuration loader | [README](../packages/octocode-config/README.md) · [Architecture](../packages/octocode-config/ARCHITECTURE.md) |
| `@octocodeai/octocode-skill-installer` | Durable cross-platform Agent Skill installation | [README](../packages/octocode-skill-installer/README.md) · [Architecture](../packages/octocode-skill-installer/ARCHITECTURE.md) |
| `@octocodeai/octocode-awareness` | Coordination, workspace history, and shared host primitives | [README](../packages/octocode-awareness/README.md) · [Architecture](../packages/octocode-awareness/ARCHITECTURE.md) · [Docs index](../packages/octocode-awareness/docs/README.md) |
| `@octocodeai/octocode-benchmark` | Research benchmarks, evals, graders, and reports | [README](../packages/octocode-benchmark/README.md) · [Architecture](../packages/octocode-benchmark/ARCHITECTURE.md) · [Results](../packages/octocode-benchmark/results/README.md) |

The separately versioned `@octocodeai/octocode-core` package owns public tool schemas, descriptions, and shared MCP/CLI instructions. See the [root package explanation](../README.md#packages) for its relationship to this monorepo.

## Agent Skills

The [`skills/`](../skills) directory contains reusable workflows for research, architecture, documentation, evaluation, prompt design, scraping, browser evidence, and orchestration. Install and inspect them through the CLI:

```bash
npx octocode skill list
npx octocode skill info octocode-research
npx octocode skill install octocode-research --platform codex --global
```

Each skill owns its operating instructions in `SKILL.md` and loads detailed references only when the task needs them.

## Benchmarks and evaluation

Benchmark methodology, questions, graders, and run artifacts live under [`packages/octocode-benchmark`](../packages/octocode-benchmark). Start with the [benchmark README](../packages/octocode-benchmark/README.md), then use the [results index](../packages/octocode-benchmark/results/README.md) for completed campaigns.
