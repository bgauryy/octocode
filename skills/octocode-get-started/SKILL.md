---
name: octocode-get-started
description: "Explain Octocode, MCP, CLI, SKILL.md files, public tools and skills, installation, configuration, and home/workspace folders. Use for new-user onboarding and basic setup questions."
---

# Get started with Octocode

Keep onboarding brief. Explain the basics below, then use the linked GitHub documentation for the details the user requests. Reuse an existing installation; do not add duplicate MCP entries or skills.

## What is what?

- **Octocode** researches local code, GitHub repositories and history, and package registries. Its tools search, read files, inspect syntax and navigate symbols.
- **MCP** lets an AI client call Octocode tools directly.
- **CLI** runs the same research tools from a terminal and manages setup.
- **Skills** teach an agent how to use tools for tasks such as research, architecture, brainstorming, documentation and code review. Installing a skill does not install its optional tools or services.
- **`SKILL.md`** is a Markdown file containing a skill's name, description and instructions. The agent reads it when the skill applies. Supporting Markdown files can hold further details.

## Install

The npm launchers require Node.js 22+. The standalone native binary needs no Node; optional skill scripts may have separate requirements.

```bash
npx -y octocode --help                       # run the CLI without installing globally
npm install -g octocode                      # optional global CLI installation
npx -y octocode install --list               # find your MCP client
npx -y octocode install --ide cursor         # example: install MCP for Cursor
npx -y octocode skill list                   # discover public skills
npx -y octocode skill install octocode-research --platform codex --global
```

Choose the actual client and skill platform. Plugins may already include MCP and skills. For project-only skills, use `--project-dir /absolute/project` instead of `--global`; this does not narrow a globally enabled plugin. Reload the client's MCP connection after setup and verify a local read. Local research does not require a GitHub login or provider key.

## Configuration and folders

- **`~/.octocode/`** holds shared settings, credentials, installed skills and caches. `octocode config home` shows the actual location; `OCTOCODE_HOME` can relocate it.
- **`<workspace>/.octocode/`** holds project settings, skill outputs and graph data. Workspace configuration follows the process working directory, without searching parent folders.
- **`.octocoderc`** contains JSONC settings; **`.env`** contains environment fallbacks and provider keys. Keep secrets out of version control.
- Settings resolve from process/MCP environment, workspace `.env`, home `.env`, workspace `.octocoderc`, home `.octocoderc`, then defaults. Some settings are home-only.
- Use `octocode config view` to inspect/edit settings and `octocode config check KEY` to check key presence. For authorized credential entry, use `octocode config set KEY --stdin`; it writes the home `.env`. Never ask the user to paste secrets into chat.

## GitHub references

| Need | Documentation |
|---|---|
| Product overview | [Octocode](https://github.com/bgauryy/octocode) |
| Every tool and how to use it | [Tools](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md) |
| Every public skill and when to use it | [Skills](https://github.com/bgauryy/octocode/blob/main/skills/README.md) |
| CLI commands and installation scopes | [CLI](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md) |
| MCP installation and connection | [MCP](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md) |
| Configuration, precedence and folders | [Configuration](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) |
| All settings, environment keys and defaults | [Settings reference](https://github.com/bgauryy/octocode/blob/main/docs/generated/CONFIG_SETTINGS.md) |
| GitHub login and provider credentials | [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md) |

## Output

Answer in chat: explain what the user needs, give the relevant commands and link the detailed reference. Report any checks actually performed; do not imply that installation or a listed tool proves a working connection. Hand code questions to `octocode-research` once setup works.
