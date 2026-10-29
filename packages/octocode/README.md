# Octocode CLI

`octocode` researches code from your terminal: local files, GitHub
repositories, pull requests and history, and package registries. It also adds
the Octocode MCP server to agent clients and installs Octocode Agent Skills.
The npm entry point launches the native Rust CLI unchanged. Both entry points use
the same commands, output, and exit codes, including skills and configuration.

## Run

Requires Node.js `^24.15.0`. Nothing to install:

```bash
npx octocode --help
```

## Quick start

```bash
npx octocode auth login                                       # optional: private repos, higher rate limits
npx octocode localSearch '{"path":".","matchString":"TODO"}'  # search this directory
npx octocode localFetch --help                                # required fields and an example for any tool
npx octocode install --ide cursor                             # add the MCP server to an agent client
npx octocode skill install --all --platform claude --global   # install the Agent Skills
```

## Commands

### Research tools

Each tool takes one JSON query: inline, from a file (`--input FILE`), or from
stdin (`--input -`). Batch up to 5 with `{"queries":[…]}`.
`npx octocode <tool> --help` lists the required fields and a runnable example;
`npx octocode schema <tool> --view query` lists every field.

| Command | Use it to |
|---|---|
| **Local** | |
| `localSearch` | Find literal or regex matches in local files. |
| `localFetch` | Read a known local file or an exact source region. |
| `structureSearch` | Outline local directories or find files by name or metadata. |
| `astSearch` | Find declarations, syntax trees, or structural code matches. |
| `lspSearch` | Resolve callers, references, same-name symbols, types, or diagnostics. |
| `graphBuild` | Build or reuse a workspace syntax snapshot (CLI only). |
| `graphQuery` | Explore that snapshot's dependencies, calls, cycles and changes (CLI only). |
| **GitHub** | |
| `ghSearchRepo` | Discover repositories by keywords, owner, or filters. |
| `ghSearchCode` | Find default-branch code or file paths in an owner or repository. |
| `ghStructure` | Browse a repository's tree, refs, or languages. |
| `ghGetFileContent` | Read a known GitHub file or an exact source region. |
| `ghSearchHistory` | Find pull requests, issues, or commits. |
| `ghGetHistoryItem` | Read a known pull request, issue, commit, or comparison. |
| `ghCloneRepo` | Cache a shallow checkout for repeated local analysis (CLI only). |
| **Packages** | |
| `artifactSearch` | Find package versions, dependencies, or upstream release source (npm, PyPI, crates, Maven, NuGet, Go, Packagist, RubyGems). |
| **Beta** (`OCTOCODE_BETA=true`) | |
| `clasify` | Typed answers about unread text, images, or held data. Also needs `OCTOCODE_CLASSIFICATION_API`. |

Each tool takes flags like rg and gh, or one JSON query:

```bash
npx octocode localSearch createServer src -w -g '*.ts'
npx octocode ghSearchHistory pullRequest -R bgauryy/octocode -s merged -L 5
npx octocode artifactSearch zod --ecosystem npm
npx octocode localFetch src/index.ts:1-40
npx octocode localSearch '{"path":"src","matchString":"createServer"}'
```

### Code graph

Build or reuse a Rust syntax snapshot, then query it repeatedly. The agent-facing
tools are CLI-only; Octocode Code invokes this native CLI through its graph tools.

```bash
npx octocode --json graphBuild '{"queries":[{"path":"."}]}'
npx octocode --json graphQuery '{"queries":[{"operation":"stats","path":"."}]}'
npx octocode schema graphQuery --view query
```

Snapshots live under the workspace's `.octocode/graph`. Query the same path used
for the build, even when pinning a returned `graph` ID. Check freshness and scan
coverage; follow returned continuations for remaining evidence. The positional
`graph ingest` / `graph query` commands remain available. See the
[graph tool reference](../octocode-native/docs/engine/CODE_GRAPH.md#agent-graph-tools-cli-and-beta-mcp)
for operation requirements, defaults and snapshot retention.

### Setup and management

| Command | Does |
|---|---|
| `schema [tool]` | List the enabled tools with agent instructions, or print one tool's input contract. |
| `config` | Show config and `.env` paths and loaded key names; `home`, `get`, `set`, `unset`, `check`, `view`. |
| `auth status\|login\|logout` | GitHub sign-in with the device flow, its status, and sign-out. |
| `install --ide <id>` | Add (or `--remove`) the Octocode MCP server in a client, at `--scope home\|workspace\|local`; `--list` shows each client's status. |
| `skill list\|info\|install\|remove\|check` | Manage the bundled Agent Skills. |
| `help [command]` | Help for any command (also `--help`, `-h`). |

Output is text on a terminal or a pipe; search hits print as rg-style
`path:line:text` lines, with counts and follow-up commands on stderr. `--json`
prints JSON (indented on a terminal, one line on a pipe). Exit `0` includes
results with more pages: run the printed `next` command.

This package is a small launcher. npm installs the one native binary package
for your platform (`@octocodeai/octocode-native-<platform>`) and the launcher
runs it.

## Documentation

- [CLI guide](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md):
  every command, flags, graph, and exit codes
- [Tool reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md)
- [Configuration](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md)
  and [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md)

## Development

From the repository root:

```bash
yarn workspace octocode build
yarn workspace octocode test
```

See [CLI architecture](https://github.com/bgauryy/octocode/blob/main/packages/octocode/ARCHITECTURE.md).

## License

MIT
