# Octocode documentation

Each topic has one owner doc; other docs link to it instead of repeating it. The root [README](../README.md) introduces the toolkit.

## Start here

| Doc | Owns |
|---|---|
| [OCTOCODE_PROTOCOL.md](OCTOCODE_PROTOCOL.md) | The concept: evidence dimensions, the research loop, how each part of the protocol works, and measured strengths and limits |
| [BENCHMARKS.md](BENCHMARKS.md) | Measured results vs gh, rg, sed and ast-grep on real repositories and PRs: context, accuracy, safety |
| [Root README quick start](../README.md#quick-start) | Installing and first run |
| [OCTOCODE_MCP.md](OCTOCODE_MCP.md) | The MCP server: client setup, registered tools, instructions, startup and lifecycle |
| [OCTOCODE_CLI.md](../packages/octocode/docs/OCTOCODE_CLI.md) | The CLI: commands, flags, output, exit codes |

## Using Octocode

| Doc | Owns |
|---|---|
| [OCTOCODE_RESEARCH_MANIFEST.md](OCTOCODE_RESEARCH_MANIFEST.md) | Choosing and combining tools for local, remote and history research; evidence boundaries |
| [OCTOCODE_TOOLS.md](OCTOCODE_TOOLS.md) | Every tool's fields, defaults, limits, results and continuations |
| [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md) | The shared request/result envelope and how evidence and `next.*` continuations carry between tools |
| [OCTOCODE_CLASIFY.md](OCTOCODE_CLASIFY.md) | `clasify`: modes, presets, limits, cache, search handoff and outputs |
| [CONFIGURATION.md](CONFIGURATION.md) | Config files, precedence, storage and caches, feature gates, troubleshooting |
| [generated/CONFIG_SETTINGS.md](generated/CONFIG_SETTINGS.md) | Generated table of every setting, env var, default and range |
| [AUTHENTICATION.md](AUTHENTICATION.md) | GitHub tokens, OAuth login and refresh, `gh` passthrough, Enterprise, the `clasify` key, npm registry credentials |
| [SECURITY.md](SECURITY.md) | Input validation, secret redaction, filesystem policy, credential protection, egress |

## Developing Octocode

| Doc | Owns |
|---|---|
| [DEVELOPMENT.md](../skills-dev/octocode-dev/docs/DEVELOPMENT.md) | Package map, contract pipeline, build/test/lint commands, dev env vars, ownership rules |
| [ADDING_CONFIG.md](../skills-dev/octocode-dev/docs/ADDING_CONFIG.md) | Adding a configuration setting, section or credential |
| [TOOL_QUALITY.md](../skills-dev/octocode-dev/docs/TOOL_QUALITY.md) | Acceptance criteria for public tool quality |
| [RELEASE.md](../skills-dev/octocode-dev/docs/RELEASE.md) | Release checklist and gates |
| [AGENTS.md](../AGENTS.md) | Repository rules for agents working in this repo |
| [skills-dev/octocode-dev/scripts/README.md](../skills-dev/octocode-dev/scripts/README.md) | Root automation scripts |

Each package also has a `README.md` (public purpose) and `ARCHITECTURE.md` (ownership and invariants); [DEVELOPMENT.md](../skills-dev/octocode-dev/docs/DEVELOPMENT.md#packages) links the package map. Benchmark campaigns live in [packages/octocode-benchmark](../packages/octocode-benchmark/README.md), local end-to-end suites in [octocode-local-testing](../octocode-local-testing/README.md).

## Skills

| Location | Owns |
|---|---|
| [skills/](../skills/README.md) | Published Agent Skills (research, architecture, documentation, evaluation, scraping, orchestration, …); each `SKILL.md` owns its workflow |
| [skills-beta/](../skills-beta/README.md) | Tested skills not yet published |
| [skills-dev/](../skills-dev/README.md) | Skills for working on this repository |

```bash
npx octocode skill list
npx octocode skill info octocode-research
npx octocode skill install octocode-research --platform codex --global
```
