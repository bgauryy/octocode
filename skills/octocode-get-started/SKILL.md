---
name: octocode-get-started
description: "Load first to install or set up Octocode: CLI, MCP server in an editor or agent (Cursor, VS Code, Claude Code, Codex), public skills, GitHub login or settings, first tool call check."
metadata:
  excluded-hosts: "octocode-code"
---

# Get started with Octocode

[Octocode](https://github.com/bgauryy/octocode) searches local code, GitHub history and package registries. The [CLI](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md) and [MCP connection](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md) expose the same research tools; [skills](https://github.com/bgauryy/octocode/blob/main/skills/README.md) guide tasks such as research or review. Identify the user's client and reuse an existing installation or plugin before adding anything.

## Install and verify

A plugin may already supply MCP and skills. npm launchers require Node.js 22+; the standalone native binary needs no Node, though optional skill scripts may.

```bash
npx -y octocode --help                       # run without a global install
npm install -g octocode                      # optional global CLI
npx -y octocode install --list               # check client IDs and existing MCP entries
npx -y octocode install --ide cursor         # if missing, use a client ID that install --list shows
npx -y octocode skill list                   # discover public skills
npx -y octocode skill install octocode-research --platform codex --global
```

Choose the actual skill platform. For a project install, replace `--global` with `--project-dir /absolute/project`. A globally enabled plugin still applies elsewhere. Reload the client's MCP connection and verify it by reading a known file **through that client**. For example, where `README.md` exists, call `localFetch` with `{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}` and check the returned line. Local reads require neither a GitHub login nor a provider key. If that call fails, confirm the entry with `install --list`, reload the client, check `node --version`, then report the exact error.

## Configure and deliver

Add only keys needed by the task. GitHub tools use `octocode auth login`; the user enters other secrets through `octocode config set <KEY> --stdin`. Keep secrets out of chat and version control. `octocode config list` shows values, sources and defaults; `config unset` restores defaults. Read [configure](references/configure.md) for folders, precedence and individual keys. [Configure](references/configure.md#add-and-configure-keys) sets the save scope and authority rules.

Answer briefly with applicable commands, references, checks performed and gaps. Installation or listing alone does not prove a working connection; require a successful call through the client. Hand code questions to `octocode-research` afterward. Save a setup note only when requested, under `<workspace>/.octocode/octocode-get-started/` (scratch in `tmp/`) unless a path was named.
