# Octocode CLI

Run research tools from your terminal or a script. The standalone `octocode` binary and the Node.js 22+ launcher `npx -y octocode` take the same arguments. Start in the project you want to research.

## Quick start

In a project with a README, read its first line. The default output is text. Add `--json` to get a result a script can parse.

```bash
npx -y octocode localFetch '{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}'
npx -y octocode --json localFetch '{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}'
```

The examples below use `octocode`. Substitute `npx -y octocode` if you use the npm launcher. An npm install (`npm install -g octocode`) whose install script ran makes the `octocode` command the native binary itself. Where the install script is skipped (`--ignore-scripts`) or leaves the Node launcher in place (Windows, Yarn 2+, a pnpm global install, a failed version check), each call starts Node first.

## Commands

| Task | Command |
|---|---|
| List research tools | `octocode schema` |
| Choose a tool for a question | `octocode schema --view flow` |
| See a tool's operations and example inputs | `octocode schema localFetch --view variants` |
| Inspect every query field or narrow to an operation | `octocode schema localFetch` or `octocode schema ghStructure --select operation=tree` |
| Interpret output fields or partial results | `octocode schema localSearch --view output` |
| Run a saved request or read one from stdin | `octocode localFetch --input request.json` or `octocode localFetch --input -` |
| Judge piped command output with `clasify` | `<cmd> 2>&1 \| octocode clasify --content - '<json>'`: stdin becomes the one row's `content`. [Details](OCTOCODE_CLASIFY.md#from-a-pipe-cli) |
| Start a stdio MCP server | `octocode mcp` |

The CLI lists 16 tools by default, including graph, clone and advisory tools. A classification key adds `clasify`. Each tool takes a JSON envelope with `queries`, or an issued `cursor` alone. MCP uses the same envelope and applies its own catalog filters. [Tool reference](OCTOCODE_TOOLS.md) · [Availability and opt-ins](CONFIGURATION.md#tool-availability-cli-and-mcp)

## Output

| Stream or format | What to read |
|---|---|
| Text on stdout | Search hits and source lines with their paths, relative to the `# root:` line when a row's files share a directory. A line number refers to the source. A batch labels rows `## results[N]` (N is the query's place) and names the row on each of its `#` lines. Lines starting with `#` report status, coverage, warnings, hints and commands for remaining pages. |
| Notes on stderr | Counts (one line per row), related leads, runtime notices and, on a terminal, progress. |
| `--json` on stdout | A result envelope with one indexed row per query or a structured call error. Keep the request with the response when consuming JSON. |

Readable text rounds fractional numeric values to four significant digits, or to the nearest integer when their magnitude is at least 10,000. For example, `0.20158` displays as `0.2016`. Integer counts and source text are unaffected. Use `--json` or the MCP JSON result when exact numeric values matter.

When output includes a continuation command, run it unchanged. First follow `responsePagination.next`, then any remaining `next.nextPage`. Other `next` entries cover specific branches such as an exact read. For scripts, use the JSON form to reconstruct pages. [Tool data contract](TOOL_DATA_CONTRACT.md#executable-continuations)

### Exit codes

| Code | Meaning |
|---:|---|
| 0 | Success. A result may have more pages. |
| 1 | All rows empty or out of range. |
| 2 | `correctInput`: the request is wrong or names something absent (a missing path, repository or ref included). Inspect the field error and the tool's schema. |
| 4 | `refused`: authentication, permission, path policy or configuration refuses it. |
| 5 | `unservable`: a valid request this setup cannot serve (unsupported language, binary or oversized file, missing language server, internal failure). |
| 6 | `restart`: the saved paging state is gone or changed; run `next.restart`. |
| 7 | `transient`: a timeout, outage or rate limit; a retry can succeed when the row says `retryable: true`. |
| 130 | Interrupted. |

Exit 3 is reserved and never used. Every error row states its `errorClass` (`correctInput`, `refused`, `unservable`, `transient`, `restart`); transient rows add a boolean `retryable` (`false` for a cancelled call). `octocode lsp install` exits 5 when it fails, and `octocode config` exits 6 on an edit conflict.

In a batch, the strictest row sets the code: any `correctInput` row exits 2, else any `refused` 4, else any `restart` 6, else any `unservable` 5, else all-transient 7. A mix of successful and empty rows exits 0. Always inspect the individual rows.

## Install the MCP server

```bash
octocode install --list
octocode install --ide cursor --dry-run
octocode install --ide cursor
octocode install --ide cursor --check
```

The default scope is `home`. Use `--scope workspace` for a supported project config or `--scope local` for Claude Code's entry for the current directory. `--remove` deletes the entry, and `--rollback <backup>` restores a backup. A custom entry requires `--force` to overwrite. `--pass-env` copies the shell's `GITHUB_TOKEN` and `OCTOCODE_ENABLE_LOCAL` into the client config, where any copied token is plain text. [MCP setup](OCTOCODE_MCP.md)

## Settings and GitHub login

| Goal | Command |
|---|---|
| Sign in and inspect the active account | `octocode auth login` then `octocode auth status --json` |
| See every setting, default and effective source | `octocode config list` |
| Inspect one value or check whether a key is configured | `octocode config get lsp.prewarm` or `octocode config check OCTOCODE_CLASSIFICATION_API` |
| Open the settings page or print its one-use local URL | `octocode config view` or `octocode config view --no-open` |
| Locate the Octocode home | `octocode config home` |

The settings page can save to home or workspace where the setting permits it. Terminal `config set` and `config unset` edit the global `.env`. [Configuration and precedence](CONFIGURATION.md) · [Authentication and logout](AUTHENTICATION.md)

## Language servers

If a symbol lookup lacks semantic results, inspect the route for the actual file. Replace the example path with one from your project.

```bash
octocode lsp-server status src/main.rs
octocode lsp-server list
octocode lsp-server install rust-analyzer
```

Managed downloads cover `rust-analyzer` and `clangd`. Use `status --live` to start and initialize the configured servers as a check. [Other languages and custom servers](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md)

## `skill` — agent skills

```bash
octocode skill list
octocode skill info octocode-research
octocode skill install octocode-research --platform codex --project-dir .
octocode skill check --json
```

Public skills are bundled with the binary. An install stores the skill under the Octocode home and links it into the selected agent's skills directory by default. Use `--global` for all projects, `--dry-run` to preview, and `skill install --help` for platforms, copy mode and upgrades. [Skill catalog](../skills/README.md)

## Clear tool caches

Read-only tool calls on macOS and Linux, from the CLI and from an MCP server on persistent storage, run on a shared warm server, `octocode serve`: one per binary build, workspace and process environment, started by the first call and exiting after 10 minutes without a request. A copy of the binary is another build with its own server; a hard link or symlink is the same one. [Warm tool server](../packages/octocode-native/README.md#warm-tool-server)

Use `octocode cache clear --json` to stop those servers after their running calls finish and clear saved pages, provider results, materializations, URL captures and the current workspace's graph extraction checkpoints. Outstanding cursors become unusable. Stop other active tool calls and restart separate MCP servers to clear their in-memory state. Clones, published graph snapshots and credentials have separate lifecycles. [Storage and retention](STORAGE_AND_LIMITS.md)
