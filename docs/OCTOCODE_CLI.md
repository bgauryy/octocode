# Octocode CLI

Run research tools from your terminal or a script. The standalone `octocode` binary and the Node.js 22+ launcher `npx -y octocode` take the same arguments. Start in the project you want to research.

## Quick start

In a project with a README, read its first line. The default output is text. Add `--json` to get a result a script can parse.

```bash
npx -y octocode localFetch '{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}'
npx -y octocode --json localFetch '{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}'
```

The examples below use `octocode`. Substitute `npx -y octocode` if you use the npm launcher.

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

When output includes a continuation command, run it unchanged. First follow `responsePagination.next`, then any remaining `next.nextPage`. Other `next` entries cover specific branches such as an exact read. For scripts, use the JSON form to reconstruct pages. [Tool data contract](TOOL_DATA_CONTRACT.md#executable-continuations)

### Exit codes

| Code | Meaning |
|---:|---|
| 0 | Success. A result may have more pages. |
| 1 | All rows empty or out of range. |
| 2 | Invalid input. Inspect the field error and the tool's schema. |
| 3 | Target not found. |
| 4 | Authentication or permission denied. |
| 5 | Execution failure, unavailable saved state, or mixed failure codes. |
| 7 | Rate limited. |
| 130 | Interrupted. |

In a batch, an invalid input row takes priority (exit 2). Otherwise failures determine the code. A mix of successful and empty rows exits 0. Always inspect the individual rows.

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

Read-only tool calls on macOS and Linux run on a shared warm server, `octocode serve`: one per binary build, workspace and configuration, started by the first call and exiting after 10 minutes without a request. [Warm tool server](../packages/octocode-native/README.md#warm-tool-server)

Use `octocode cache clear --json` to stop those servers after their running calls finish and clear saved pages, provider results, materializations, URL captures and the current workspace's graph extraction checkpoints. Outstanding cursors become unusable. Stop other active tool calls and restart separate MCP servers to clear their in-memory state. Clones, published graph snapshots and credentials have separate lifecycles. [Storage and retention](STORAGE_AND_LIMITS.md)
