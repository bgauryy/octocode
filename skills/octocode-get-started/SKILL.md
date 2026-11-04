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

Each value resolves in order from process/MCP environment, workspace `.env`, home `.env`, workspace `.octocoderc`, home `.octocoderc`, then its default. Keys belong in `.env`; settings belong in `.octocoderc`, JSON that accepts comments. A workspace file cannot set a home-only value. [Configuration](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) · [All settings and defaults](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md#all-settings-and-defaults)

## Add and configure keys

Local tools need no keys. Add only what the user's task needs:

| To use | Set | Default |
|---|---|---|
| GitHub tools | `octocode auth login`, or `GH_TOKEN` / `GITHUB_TOKEN` | none |
| `clasify` and inline `classification` | `OCTOCODE_CLASSIFICATION_API`, a key from the [provider](https://docs.typesafe.ai/introduction) | unset (off) |
| A proxy or other provider for classification | `OCTOCODE_CLASSIFICATION_API_HOST`; HTTPS API root, home-only | `https://api.typesafe.ai` (Jev) |
| Web search in `octocode-brainstorming` | `TAVILY_API_KEY`, `EXA_API_KEY` or `SERPER_API_KEY` | unset |
| `octocode-scraping` | `SCRAPING_ANT` | unset |

Ask before saving anything, and have the user enter a secret themselves so it stays out of chat and version control:

```bash
octocode config set OCTOCODE_CLASSIFICATION_API --stdin    # the key, read from stdin into the home .env
octocode config set classification.apiHost https://gateway.example.com    # optional: another host
octocode config check OCTOCODE_CLASSIFICATION_API          # exit 0 set, 1 unset; prints no value
octocode config get classification.apiHost                 # the host in use
octocode config unset classification.apiHost               # back to the default
octocode config list                                       # every setting: value, source, default
```

On a terminal, `octocode config` opens a private local page for settings, keys and agent entries. Restart the MCP server or start a new session after any change, then confirm with a call that uses the key. [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md)

## Output

Answer the setup question briefly in chat with the applicable commands, reference and checks actually performed. A working connection needs a successful call. Hand code questions to `octocode-research` once setup works.
