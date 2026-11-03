---
name: octocode-get-started
description: "Explain Octocode, MCP, CLI, SKILL.md files, public tools and skills, installation, configuration, and home/workspace folders. Use for new-user onboarding and basic setup questions."
metadata:
  excluded-hosts: "octocode-code"
---

# Get started with Octocode

[Octocode](https://github.com/bgauryy/octocode) searches local code, GitHub repositories and history, and package registries. Its [tools](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md) read files, inspect syntax and navigate symbols. An AI client calls them through [MCP](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md). The [CLI](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md) runs the same research tools in a terminal and manages setup.

[Skills](https://github.com/bgauryy/octocode/blob/main/skills/README.md) guide an agent through tasks such as research or code review. Each `SKILL.md` gives the name, description and instructions the agent reads when the skill applies. Supporting files hold details. Optional tools and services have their own installation requirements.

## Install and verify

Reuse the existing installation. A plugin may already supply both MCP and skills. The npm launchers require Node.js 22+. The standalone native binary runs without Node, though optional skill scripts may need it.

```bash
npx -y octocode --help                       # run without a global install
npm install -g octocode                      # optional global CLI
npx -y octocode install --list               # check client IDs and existing MCP entries
npx -y octocode install --ide cursor         # if missing, use the actual client ID
npx -y octocode skill list                   # discover public skills
npx -y octocode skill install octocode-research --platform codex --global
```

Choose the actual skill platform. For a project install, replace `--global` with `--project-dir /absolute/project`. A globally enabled plugin still applies elsewhere. Reload the client's MCP connection and verify it by reading a known file **through that client**. For example, where `README.md` exists, call `localFetch` with `{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}` and check the returned line. Local reads require neither a GitHub login nor a provider key.

## Configuration and folders

| Location | Contents |
|---|---|
| `~/.octocode/` by default | Shared settings, credentials, installed skills and caches. `octocode config home` shows the actual path. `OCTOCODE_HOME` relocates it. |
| `<workspace>/.octocode/` | Project settings, skill outputs and graph data. Configuration uses the process working directory without searching parents. |

Settings resolve in order from process/MCP environment, workspace `.env`, home `.env`, workspace `.octocoderc`, home `.octocoderc`, then defaults. The `.octocoderc` files hold JSONC settings. Some settings are home-only. [Configuration](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) · [Settings and defaults](https://github.com/bgauryy/octocode/blob/main/docs/generated/CONFIG_SETTINGS.md)

Use `octocode config view` to inspect or edit settings and `octocode config check KEY` for key presence. With authorization, `octocode config set KEY --stdin` writes credentials to the home `.env`. Keep secrets out of chat and version control. [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md)

## Output

Answer the setup question briefly in chat with the applicable commands, reference and checks actually performed. A working connection needs a successful call. Hand code questions to `octocode-research` once setup works.
