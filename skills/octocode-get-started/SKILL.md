---
name: octocode-get-started
description: "Load this skill first when the user wants to install or configure the Octocode CLI, connect octocode-mcp to an agent, add skills, or sign in to GitHub. Verify setup with a real tool call."
metadata:
  excluded-hosts: "octocode-code"
---

# Get started with Octocode

Use `octocode` for terminal commands, configuration, and GitHub sign-in. Use `octocode-mcp` when an editor or agent needs the tools over MCP. A skill teaches the agent a workflow. Check which client and project the user wants to set up and whether a plugin already provides the connection.

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

## Choose tools and skills

| Task | Start here |
|---|---|
| Find and read local code | `structureSearch` or `localSearch` → `localFetch`. Use `astSearch` for syntax and `lspSearch` for callers or definitions. |
| Read GitHub code or history | `ghSearchRepo` → `ghStructure` or `ghSearchCode` → `ghGetFileContent`. For a change, use `ghSearchHistory` → `ghGetHistoryItem`. |
| Check a dependency | Resolve the lockfile version with `artifactSearch`. Use `securityCheck` for known advisories. |
| Explore a repository locally | `ghCloneRepo` creates a checkout. `graphBuild` → `graphQuery` finds dependencies, cycles, and change impact. |
| Filter results by meaning | `clasify` answers questions about local or GitHub reads, images, web pages, and supported search results. A fresh evaluation sends evidence to the configured provider and can incur charges. |

MCP starts with 12 tools. The CLI starts with 16. Use `npx -y octocode schema` for the CLI catalog or the client’s tool list for MCP. [Enable optional tools](references/configure.md#optional-tools)

Run `npx -y octocode skill list` to see the bundled workflows. For example, install code research in the current Codex project with `npx -y octocode skill install octocode-research --platform codex --project-dir .`. Use the user’s platform and requested scope. [Skill catalog](https://github.com/bgauryy/octocode/blob/main/skills/README.md)

Report the interface, project, and checks that succeeded. For a failed check, give the failing command or tool, the error, and which part of setup it affects. Keep tokens and device codes out of the report.
