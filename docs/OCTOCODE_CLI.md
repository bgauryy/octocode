# Octocode CLI

Run research tools from your terminal or a script. The standalone `octocode` binary and the Node.js 22+ launcher `npx -y octocode` take the same arguments. Run commands from the project you want to research.

## Quick start

In a project with a README, read its first line. Output is text by default. Add `--json` for output a script can parse.

```bash
octocode localFetch '{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}'
octocode --json localFetch '{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}'
```

The examples use `octocode`. Replace it with `npx -y octocode` if you use the launcher.

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

| Stream or format | What to read |
|---|---|
| Text on stdout | Hits and source lines with their paths. When a row's paths share a directory, it prints once as `# root: <dir>` and the paths after it are relative to it: join the root, `/` and a printed path to get the path another call takes (a lone path, or paths with no common directory, print whole). A line number refers to the source file. Each batch row starts with `## results[N]`, where N is the query's position. Lines starting with `#` give status, coverage, warnings, hints and the command for the next page. |
| Notes on stderr | One counter line per row, related leads and runtime notices. |
| `--json` on stdout | A result envelope with one indexed row per query, or a structured error. Keep the request beside the response when you save JSON. |

Text output rounds fractions to four significant digits. Values of 10,000 or more round to an integer. For example, `0.20158` prints as `0.2016`. Use `--json` when exact values matter.

When output includes a continuation command, run it unchanged. A reply that ends with a `# next:` command is unfinished: run it until none is printed. A row's `next.*` entries cover specific branches, such as an exact read. [Tool data contract](TOOL_DATA_CONTRACT.md#executable-continuations)

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

Exit 3 is never used. Every error row states its `errorClass`. A batch exits with the strictest row's code, in this order: 2, 4, 6, 5, then 7. A mix of successful and empty rows exits 0, so read each row.

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

Managed installs cover `rust-analyzer` and `clangd`. `lsp-server status --live` also starts the configured servers and completes their `initialize` step. [Other languages and custom servers](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md)

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

`cache clear` stops background tool servers after their running calls finish. It then removes saved pages, provider answers, URL captures, materialized files and this workspace's graph checkpoints. Cursors issued before the clear may stop working. Clones, published graphs, credentials and installed language servers are kept. [Storage and limits](STORAGE_AND_LIMITS.md)
