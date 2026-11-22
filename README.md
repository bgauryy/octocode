# Octocode

<div align="center">
  <img src="https://github.com/bgauryy/octocode/raw/main/packages/octocode-mcp/assets/logo_white.png" width="400px" alt="Octocode Logo">

  [![MCP Community Server](https://img.shields.io/badge/Model_Context_Protocol-Official_Community_Server-blue?style=flat-square)](https://github.com/modelcontextprotocol/servers)
  [![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/bgauryy/octocode)
  [![Glama score](https://glama.ai/mcp/servers/bgauryy/octocode/badges/score.svg)](https://glama.ai/mcp/servers/bgauryy/octocode)

  [![Website](https://img.shields.io/badge/Website-007ACC?style=for-the-badge&logo=link&logoColor=white)](https://octocode.ai)
  [![YouTube](https://img.shields.io/badge/YouTube-FF0000?style=for-the-badge&logo=youtube&logoColor=white)](https://www.youtube.com/@Octocode-ai)

</div>

Octocode helps you answer questions about code and show where the answer came from. Trace a caller to its source line, find the pull request behind a change, or check whether an advisory affects the version in your lockfile. Use the same tools in your coding agent over MCP or from the terminal.

[Install](#install) · [Tools](#tools) · [Workflows](#how-an-agent-uses-octocode) · [Configuration](#configuration) · [Skills](#skills)

## Install

The npm launchers require Node.js 22 or later. Prebuilt binaries cover macOS on Apple Silicon or Intel, Linux on x64 or ARM64 (glibc, or musl such as Alpine), and Windows on x64 or ARM64. The standalone `octocode` binary runs without Node.js. [Native installation](packages/octocode-native/README.md#install)

| Interface | Start here | Check that it works |
|---|---|---|
| MCP in your agent | Run `npx -y octocode install` to choose a client, or `npx -y octocode install --ide cursor` for Cursor. Restart the client. | Open a project with a README and ask “Use Octocode to read the first line of README.md.” |
| CLI in a terminal or script | Run `npx -y octocode --help` from the project directory. | From a project with a README, run the command below. The result includes its first line. |

```bash
npx -y octocode localFetch '{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}'
```

For GitHub code search or private repositories, sign in on the machine that runs Octocode. Run `npx -y octocode auth login`, open the displayed URL, enter the device code, then run `npx -y octocode auth status`. Octocode can also use an existing GitHub CLI login or a token from your environment. [GitHub authentication and token precedence](docs/AUTHENTICATION.md)

[<img src="https://cursor.com/deeplink/mcp-install-dark.svg" alt="Install in Cursor" height="20">](https://cursor.com/en/install-mcp?name=octocode&config=eyJjb21tYW5kIjoibnB4IiwidHlwZSI6InN0ZGlvIiwiYXJncyI6WyIteSIsIm9jdG9jb2RlLW1jcEBsYXRlc3QiXX0=) [<img src="https://img.shields.io/badge/VS_Code-Install_Server-0098FF?style=flat-square&logo=visualstudiocode&logoColor=white" alt="Install in VS Code">](https://insiders.vscode.dev/redirect/mcp/install?name=octocode&config=%7B%22command%22%3A%22npx%22%2C%22type%22%3A%22stdio%22%2C%22args%22%3A%5B%22-y%22%2C%22octocode-mcp%40latest%22%5D%7D)

<details>
<summary>More one-click installs</summary>

[<img src="https://img.shields.io/badge/VS_Code_Insiders-Install_Server-24bfa5?style=flat-square&logo=visualstudiocode&logoColor=white" alt="Install in VS Code Insiders">](https://insiders.vscode.dev/redirect/mcp/install?name=octocode&config=%7B%22command%22%3A%22npx%22%2C%22type%22%3A%22stdio%22%2C%22args%22%3A%5B%22-y%22%2C%22octocode-mcp%40latest%22%5D%7D&quality=insiders)
[<img src="https://img.shields.io/badge/Windsurf-Install_Server-1a1a1a?style=flat-square&logoColor=white" alt="Install in Windsurf">](windsurf://mcp/install?name=octocode&config=%7B%22command%22%3A%22npx%22%2C%22type%22%3A%22stdio%22%2C%22args%22%3A%5B%22-y%22%2C%22octocode-mcp%40latest%22%5D%7D)
[<img src="https://kiro.dev/images/add-to-kiro.svg" alt="Install in Kiro" height="20">](https://kiro.dev/launch/mcp/add?name=octocode&config=%7B%22command%22%3A%22npx%22%2C%22type%22%3A%22stdio%22%2C%22args%22%3A%5B%22-y%22%2C%22octocode-mcp%40latest%22%5D%7D)
[<img src="https://goose-docs.ai/img/extension-install-dark.svg" alt="Install in Goose" height="20">](https://goose-docs.ai/extension?cmd=npx&arg=-y&arg=octocode-mcp%40latest&id=octocode&name=octocode&description=Evidence-first%20code%20research%20for%20AI%20agents)
[<img src="https://files.lmstudio.ai/deeplink/mcp-install-light.svg" alt="Install in LM Studio" height="20">](https://lmstudio.ai/install-mcp?name=octocode&config=eyJjb21tYW5kIjoibnB4IiwidHlwZSI6InN0ZGlvIiwiYXJncyI6WyIteSIsIm9jdG9jb2RlLW1jcEBsYXRlc3QiXX0=)

</details>

## Tools

**17 tools.** With default settings you get 12 tools over MCP and 16 in the CLI. Both interfaces use the same tool names and JSON inputs.

| Task | Tools | Availability |
|---|---|---|
| Find and read local code | `structureSearch` finds files, `localSearch` finds text, and `localFetch` reads the matching lines. | Default |
| Follow a symbol | `astSearch` matches code by syntax. `lspSearch` finds definitions, references and callers. | Default |
| Read GitHub code | `ghSearchRepo` finds repositories. `ghSearchCode` finds code on the default branch. `ghStructure` browses a tree at a ref, and `ghGetFileContent` reads a file there. | Default |
| Trace a change | `ghSearchHistory` finds commits, pull requests and issues. `ghGetHistoryItem` opens their patches or discussions and compares revisions. | Default |
| Look up a package | `artifactSearch` resolves a package version to registry metadata, dependencies and upstream source when available. | Default |
| Explore dependencies | `graphBuild` creates a snapshot. `graphQuery` finds importers, cycles and files a change could affect. | CLI default · MCP opt-in |
| Search a GitHub repo locally | `ghCloneRepo` returns a checkout path for local tools. Requires Git and persistent storage. | CLI default · MCP opt-in |
| Check known vulnerabilities | `securityCheck` looks up advisories for a package version, repository or commit and reports affected ranges and known fixes. | CLI default · MCP opt-in |
| Read by meaning | `clasify` answers questions about files, images and web pages, or filters search results by meaning. | Provider key on both |

Run `npx -y octocode schema` to list the tools available in your CLI. For a tool’s inputs and examples, try `npx -y octocode schema localFetch --view variants`. [Tool reference](docs/OCTOCODE_TOOLS.md)

## How an agent uses Octocode

| Ask your agent | How it can check the answer |
|---|---|
| “Who calls this function?” | Find its declaration with `localSearch` or `astSearch`. Give the file, line and symbol to `lspSearch`, then use `localFetch` to read a returned call site. |
| “What could break if we change this module?” | Run `graphBuild` on the project, then `graphQuery` on that snapshot for importers or impact. Read the affected code before deciding what to test. |
| “Which change introduced this behavior in our dependency?” | Give `artifactSearch` the locked version. For GitHub source, browse the release ref with `ghStructure`, read the file at the returned commit with `ghGetFileContent`, then follow `ghSearchHistory` to the patch in `ghGetHistoryItem`. |
| “Does this advisory affect the version we ship?” | Pass the package ecosystem, name and locked version to `securityCheck`. Check the advisory’s affected range and any fixed version against the lockfile. |
| “Which docs or screenshots explain retries?” | Give `clasify` a `localFetch` or `ghGetFileContent` source for a file or image, a URL for a web page, or a supported search query with a predicate. Follow `next.read` when offered to inspect the evidence. |

For repeated searches across a remote repo, run `ghCloneRepo` once and give its returned path to local tools. For a paged result, follow the returned `next` call to reach the remaining evidence. [Graph examples](docs/OCTOCODE_GRAPH.md) · [Classification examples and request budgets](docs/OCTOCODE_CLASIFY.md) · [Reading paged results](docs/TOOL_DATA_CONTRACT.md#executable-continuations)

## Configuration

Run `npx -y octocode config list` to see effective settings, defaults and sources. Octocode reads project settings from `.octocode/` in its launch directory. CLI and MCP share user settings in the Octocode home, normally `~/.octocode/`. Run `npx -y octocode config home` to see its location.

| To enable | Run once for your user |
|---|---|
| Graph tools in MCP | `npx -y octocode config set local.beta true` |
| Repository cloning in MCP | `npx -y octocode config set tools.enableClone true` |
| Advisory lookups in MCP | `npx -y octocode config set tools.enableSecurityCheck true` |
| Classification in CLI and MCP | `npx -y octocode config set OCTOCODE_CLASSIFICATION_API --stdin` |

Restart MCP after a settings change. For GitHub authentication, a process token takes precedence over a project or user `.env` token, followed by the host’s saved Octocode, OS or GitHub CLI login. [Settings, scopes and workspace permissions](docs/CONFIGURATION.md) · [GitHub token resolution](docs/AUTHENTICATION.md#github-token-resolution)

## Security

Local tools enforce workspace path rules and redact recognized secrets from text. GitHub and registry lookups contact their respective services. Classification sends selected evidence, including supplied image pixels, to your configured provider. Fresh evaluations can incur charges. [Security and advisory requests](docs/SECURITY.md) · [Storage and caches](docs/STORAGE_AND_LIMITS.md) · [Privacy](PRIVACY.md)

## Skills

Skills give your agent a workflow for a task. Run `npx -y octocode skill list` to see the bundled skills and their installation status. To add code research to a Codex project, run this from its root.

```bash
npx -y octocode skill install octocode-research --platform codex --project-dir .
```

| What you want to do | Public skills |
|---|---|
| Get set up and investigate code | [octocode-get-started](skills/octocode-get-started/SKILL.md), [octocode-research](skills/octocode-research/SKILL.md), [octocode-debugging](skills/octocode-debugging/SKILL.md) |
| Explore dependencies and visualize a repository | [octocode-graph](skills/octocode-graph/SKILL.md), [octocode-graph-view](skills/octocode-graph-view/SKILL.md) |
| Evaluate ideas and design changes | [octocode-brainstorming](skills/octocode-brainstorming/SKILL.md), [octocode-engineer](skills/octocode-engineer/SKILL.md), [octocode-rfc-generator](skills/octocode-rfc-generator/SKILL.md) |
| Review, test and document code | [octocode-pr-review](skills/octocode-pr-review/SKILL.md), [octocode-test-plan](skills/octocode-test-plan/SKILL.md), [octocode-roast](skills/octocode-roast/SKILL.md), [octocode-clean-agentic-code](skills/octocode-clean-agentic-code/SKILL.md), [octocode-documentation](skills/octocode-documentation/SKILL.md) |
| Research the web or operate a browser | [octocode-scraping](skills/octocode-scraping/SKILL.md), [octocode-chrome-devtools](packages/octocode-chrome-devtools/skills/octocode-chrome-devtools/SKILL.md) |
| Build agent workflows and coordinate work | [octocode-agentic](skills/octocode-agentic/SKILL.md), [octocode-skills](skills/octocode-skills/SKILL.md), [octocode-workflows](skills/octocode-workflows/SKILL.md), [octocode-orchestrator](skills/octocode-orchestrator/SKILL.md), [octocode-messages-proxy](skills/octocode-messages-proxy/SKILL.md) |

[Skill descriptions and installation options](skills/README.md)

## Documentation

[User guides and reference](docs/README.md) · [CLI](docs/OCTOCODE_CLI.md) · [MCP client setup](docs/OCTOCODE_MCP.md) · [Tool catalog](docs/OCTOCODE_TOOLS.md) · [Language support](packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md)

[VS Code extension](packages/octocode-vscode/README.md) · [Claude Code plugin](packages/octocode-claude-plugin/README.md) · [Codex plugin](packages/octocode-codex-plugin/README.md) · [Benchmarks](packages/octocode-benchmark/README.md) · [Contributing](AGENTS.md) · [MIT license](LICENSE)
