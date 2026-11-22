# Octocode CLI

Run these commands from your project directory. Use the standalone `octocode` binary or substitute `npx -y octocode` with Node.js 22 or later.

## Quick start

In a project with a README, read its first line as JSON. Omit `--json` for text output.

```bash
octocode --json localFetch '{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}'
```

## Commands

| Task | Command |
|---|---|
| List research tools | `octocode schema` |
| Choose a tool for a question | `octocode schema --view flow` |
| See a tool's operations and an example | `octocode schema localFetch --view variants` |
| Show every query field, or narrow to one operation | `octocode schema localFetch` or `octocode schema ghStructure --select operation=tree` |
| Read what output fields mean | `octocode schema localSearch --view output` |
| Run a saved request, or read one from stdin | `octocode localFetch --input request.json` or `octocode localFetch --input -` |
| Judge piped command output with `clasify` | `<cmd> 2>&1 \| octocode clasify --content - '<json>'`. [Details](OCTOCODE_CLASIFY.md#from-a-pipe-cli) |
| Run the MCP server over stdio | `octocode mcp` |

The CLI lists 16 tools by default, including graph, clone and security tools. A classification key adds `clasify`. Each call takes a JSON envelope with `queries`, or an issued `cursor` alone. `octocode <tool> --help` shows examples for every form of that tool. [Tool reference](OCTOCODE_TOOLS.md) · [Availability and opt-ins](CONFIGURATION.md#tool-availability-cli-and-mcp)

## Output

The default text view shows paths, hits and source lines. When paths share a directory, `# root` names it. Progress notes go to stderr. Use `--json` for exact values in a script.

If the output ends with a `# next:` command, run it to read the next page. [Read tool results](TOOL_DATA_CONTRACT.md#executable-continuations)

### Exit codes

| Code | Meaning |
|---:|---|
| 0 | Success. The result may have more pages. |
| 1 | Every row is empty or out of range. |
| 2 | `correctInput`: the request is wrong or names something that does not exist. Read the field error and the tool's schema. |
| 4 | `refused`: authentication, permission, path policy or configuration refuses the call. |
| 5 | `unservable`: a valid request this setup cannot serve, such as an unsupported language, a binary or oversized file, or a missing language server. |
| 6 | `restart`: the saved paging state is gone or changed. Run `next.restart`. |
| 7 | `transient`: a timeout, outage or rate limit. Retry when the row says `retryable: true`. |
| 130 | Interrupted. |

For a batch, check each row's status. A mix of successful and empty rows exits 0.

## Install the MCP server

```bash
octocode install --list
octocode install --ide cursor --dry-run
octocode install --ide cursor
octocode install --ide cursor --check
```

The default scope is `home`. Use `--scope workspace` for a project config, or `--scope local` for Claude Code's entry for the current directory. `--remove` deletes the entry. `--rollback <FILE>` restores a `.bak` file that an earlier install left. An entry that differs from the one Octocode writes needs `--force` to replace it.

`--pass-env` copies `GITHUB_TOKEN` and `OCTOCODE_ENABLE_LOCAL` from your shell into the client config. The token is then stored in plain text. [MCP server](OCTOCODE_MCP.md)

## Settings and GitHub login

| Goal | Command |
|---|---|
| Sign in, then check the active account | `octocode auth login`, then `octocode auth status --json` |
| See every setting, its value and its source | `octocode config list` |
| Read one value, or check whether a key is set | `octocode config get lsp.prewarm` or `octocode config check OCTOCODE_CLASSIFICATION_API` |
| Open the settings page, or print its local URL | `octocode config view`, or `octocode config view --no-open` |
| Find the Octocode home | `octocode config home` |

`config set KEY VALUE` and `config unset KEY` edit the global `.env`. Use `config set KEY --stdin` to keep a secret out of shell history. [Configuration](CONFIGURATION.md) · [Authentication](AUTHENTICATION.md)

## Language servers

If a symbol lookup returns no semantic results, check the server route for that file. Replace the example path with a file from your project.

```bash
octocode lsp-server status src/main.rs
octocode lsp-server list
octocode lsp-server install rust-analyzer
```

For a custom server, configure `lsp-servers.json` in your Octocode home. Use `lsp-server status --live` to check that the server starts. [Language server configuration](CONFIGURATION.md#set-a-value)

## `skill` — agent skills

```bash
octocode skill list
octocode skill info octocode-research
octocode skill install octocode-research --platform codex --project-dir .
octocode skill check --json
```

Skills ship inside the binary. `skill install` links each one into the chosen agent's skills directory (`--mode symlink` is the default). Add `--global` for every project, and `--dry-run` to preview. `skill install --help` lists platforms, copy mode and `--upgrade`. [Skill catalog](../skills/README.md)

## Clear tool caches

```bash
octocode cache clear --json
```

Run this from the workspace whose graph checkpoints you want to clear. It also clears cached results in the Octocode home and can invalidate saved cursors. [What the command removes and keeps](STORAGE_AND_LIMITS.md#clear-cached-work)
