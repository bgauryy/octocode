---
name: octocode-get-started
description: "Load this skill first when setting up Octocode. Install the CLI or bundled skills, connect an MCP client, configure GitHub access and verify the chosen interface with a real call."
metadata:
  excluded-hosts: "octocode-code"
---

# Get started with Octocode

Identify the client, project and requested setup. Check whether a plugin already supplies MCP. Use the CLI for terminal access and GitHub login. Use MCP for tools inside an editor or agent. Follow the branch the user needs.

## Try the CLI

The npm launchers need Node.js 22 or later. The standalone native CLI runs without Node.js. From a project with a README, run:

```sh
npx -y octocode --version
npx -y octocode localFetch '{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}'
```

The reply should contain the first line of that project’s README. Local reads work before sign-in. For a global CLI, use `npm install -g octocode`, then run the same commands as `octocode`. [CLI guide](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md)

## Connect an MCP client

1. Run `npx -y octocode install --list` to find the client ID. For Cursor, check its existing entry with `npx -y octocode install --ide cursor --check`.
2. If the entry is missing, run `npx -y octocode install --ide cursor`, substituting the user’s client ID. For a project-specific entry, add `--scope workspace` on clients that support it. For manual setup, use the [stdio configuration](references/configure.md#manual-mcp-configuration).
3. Restart the client, open the project, and ask **“Use Octocode to read the first line of README.md.”** Verify that the client calls `localFetch` and returns the same line as the CLI. If the files differ, inspect the MCP working directory and `WORKSPACE_ROOT`.

## Sign in to GitHub through the CLI

On the machine running Octocode, start the login from an interactive terminal.

```sh
npx -y octocode auth login
npx -y octocode auth status --json
```

Have the user open the displayed URL and enter the one-time code there. Check for `verification: "verified"`, the expected `hostname`, and the expected `username`. A selected device login reports `tokenSource: "octocode-storage"`. MCP can use that login when it shares the CLI’s Octocode home and GitHub host. Code search and private repositories require authentication. For CI tokens, existing GitHub CLI accounts, or Enterprise, see [GitHub credential selection](references/configure.md#github-credential-selection).

## Configure settings or add skills

| Need | Action |
|---|---|
| Choose a tool after setup | Read [first tool calls](references/configure.md#choose-a-tool) or inspect the active catalog with `npx -y octocode schema` (CLI) or the client's tool list (MCP). |
| Change settings, enable tools or add a provider key | Check [configuration precedence](references/configure.md#configuration-and-folders), [optional tools](references/configure.md#optional-tools) or [key setup](references/configure.md#add-and-configure-keys). |
| Install a bundled skill | Run `npx -y octocode skill list`. For research in a Codex project, run `npx -y octocode skill install octocode-research --platform codex --project-dir .`. Substitute the requested skill, platform and scope. [Skill catalog](https://github.com/bgauryy/octocode/blob/main/skills/README.md) |

Report the interface, project, and checks that succeeded. For a failed check, give the failing command or tool, the error, and which part of setup it affects. Keep tokens and device codes out of the report.
