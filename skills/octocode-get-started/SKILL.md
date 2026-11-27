---
name: octocode-get-started
description: "Load this skill first when setting up Octocode: install the CLI or bundled skills, connect an MCP client, sign in to GitHub and verify with a real call."
metadata:
  excluded-hosts: "octocode-code"
---

# Get started with Octocode

Identify the client, project and requested setup; ask when the client is unknown. Run only needed sections (CLI: terminal and GitHub login; MCP: tools inside a client). Full setup order: CLI, MCP client, GitHub sign-in; done when MCP step 3 returns the README line and `auth status --json` shows `verified`.

## Try the CLI

The npm launchers need Node.js 22 or later; the standalone native CLI needs none. From a project with a README, run:

```sh
npx -y octocode --version
npx -y octocode localFetch '{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}'
```

The reply should hold the README’s first line; local reads work before sign-in. For a global CLI, `npm install -g octocode`, then drop `npx -y`. [CLI guide](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md)

## Connect an MCP client

1. `npx -y octocode install --list` shows each client ID and whether its Octocode entry exists (a plugin may supply it); check an existing one with `npx -y octocode install --ide <client-id> --check`.
2. If the entry is missing, run `npx -y octocode install --ide <client-id> --enable-local true` (`--scope workspace` for a project entry where supported; `--dry-run` previews). `--enable-local true` lists the tools that search and read code on this machine; without it the server lists the GitHub and package tools only. For a client with no ID, use the [stdio configuration](references/configure.md#manual-mcp-configuration).
3. Restart the client, open the project, and ask **“Use Octocode to read the first line of README.md.”** It should call `localFetch` and return the CLI’s line. On failure, do not reinstall: no Octocode tools → rerun `--check`; GitHub tools but no `localFetch` → the entry lacks `OCTOCODE_ENABLE_LOCAL=true` (rerun step 2 with `--force`); then the client’s MCP log names the start error; a different line or missing file → set the server’s working directory or `WORKSPACE_ROOT` (stdio configuration), restart, ask again.

## Sign in to GitHub

Code search and private repositories need a login. On the machine running Octocode, start it from an interactive terminal.

```sh
npx -y octocode auth login
npx -y octocode auth status --json
```

The user opens the displayed URL and enters the one-time code. Check for `verification: "verified"` and the expected `hostname` and `username`; a device login reports `tokenSource: "octocode-storage"`, which MCP shares when it uses the same Octocode home and GitHub host. CI tokens, GitHub CLI accounts or Enterprise: [credential selection](references/configure.md#github-credential-selection).

## Configure settings or add skills

| Need | Action | Check |
|---|---|---|
| Choose a tool | [First tool calls](references/tools-and-keys.md#choose-a-tool) | `npx -y octocode schema` or the client’s tool list |
| Change settings or enable tools | [Precedence](references/configure.md#configuration-and-folders), [optional tools](references/tools-and-keys.md#optional-tools); restart MCP after | `npx -y octocode config list` |
| Add a provider key | [Key setup](references/tools-and-keys.md#add-and-configure-keys); the user enters the secret on stdin | `npx -y octocode config check <KEY>` (exit 0 when set) |
| Install a bundled skill | `npx -y octocode skill list`, then `npx -y octocode skill install <name> --platform <platform> --project-dir .` (or `--global`) | `npx -y octocode skill check <name>` |

Report the interface, project and passing checks, or the failing command or tool, its error and the setup part it affects. Omit tokens and device codes.
