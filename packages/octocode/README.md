# Octocode CLI

`octocode` researches code from your terminal: local files, GitHub
repositories, pull requests and history, and package registries. It also adds
the Octocode MCP server to agent clients and installs Octocode Agent Skills.
The npm entry point launches the native Rust CLI unchanged. Both entry points use
the same commands, output, and exit codes, including skills and configuration.

## Run

The npm CLI launcher requires Node.js `>=22`. The standalone native
`octocode` binary does not require Node.js. Optional skill scripts can have
their own dependencies; follow the relevant skill's setup instructions.
Nothing to install:

```bash
npx octocode --help
```

## Quick start

```bash
npx octocode auth login                                       # optional: private repos, higher rate limits
npx octocode localSearch '{"queries":[{"path":".","matchString":["TODO"]}]}' # search this directory
npx octocode localFetch --help                                # required fields and an example for any tool
npx octocode install --ide cursor                             # add the MCP server to an agent client
npx octocode skill install --all --platform claude --global   # install the Agent Skills
```

## Commands

### Research tools

All 16 tool commands are listed and enabled in the CLI. MCP opt-ins and catalog
filters do not hide CLI commands; missing credentials or providers fail the
call that needs them. Path and storage policies still apply. See the
[configuration matrix](../../docs/CONFIGURATION.md#tool-availability-cli-and-mcp).

Each tool takes a JSON envelope inline or from a file (`--input FILE`). Start
with `{"queries":[…]}`: up to five independent rows, or one for `graphBuild`.
Resume saved work with the emitted `{"cursor":"…"}` input unchanged.
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
| `graphBuild` | Build or reuse a workspace syntax snapshot. |
| `graphQuery` | Explore that snapshot's dependencies, calls, cycles and changes. |
| **GitHub** | |
| `ghSearchRepo` | Discover repositories by keywords, owner, or filters. |
| `ghSearchCode` | Find default-branch code or file paths in an owner or repository. |
| `ghStructure` | Browse a repository's tree, refs, or languages. |
| `ghGetFileContent` | Read a known GitHub file or an exact source region. |
| `ghSearchHistory` | Find pull requests, issues, or commits. |
| `ghGetHistoryItem` | Read a known pull request, issue, commit, or comparison. |
| `ghCloneRepo` | Cache a shallow checkout for repeated local analysis. |
| **Packages** | |
| `artifactSearch` | Find package versions, dependencies, or upstream release source (npm, PyPI, crates, Maven, NuGet, Go, Packagist, RubyGems). |
| **Classification** | |
| `clasify` | Typed answers about unread text, images, or held data using a configured provider. |

Research-tool fields go inside the JSON envelope; there are no positional query
shorthands or field flags:

```bash
npx octocode localSearch '{"queries":[{"path":"src","matchString":["createServer"],"wholeWord":true}]}'
npx octocode ghSearchHistory '{"queries":[{"operation":"pullRequest","owner":"bgauryy","repo":"octocode","filters":{"isMerged":true}}]}'
npx octocode artifactSearch '{"queries":[{"operation":"resolve","ecosystem":"npm","packageName":"zod"}]}'
npx octocode localFetch '{"queries":[{"path":"src/index.ts","ranges":[{"startLine":1,"endLine":40}]}]}'
```

### Code graph

Build or reuse a syntax snapshot, then query it repeatedly. `graphBuild` and
`graphQuery` use the same envelope in CLI and MCP; configuration owns the
[MCP opt-in](../../docs/CONFIGURATION.md#tool-availability-cli-and-mcp).

```bash
npx octocode --json graphBuild '{"queries":[{"path":"."}]}'
npx octocode --json graphQuery '{"queries":[{"operation":"stats","path":"."}]}'
npx octocode schema graphQuery --view query
```

Snapshots live under the workspace's `.octocode/graph`. Query the same path used
for the build, even when pinning a returned `graph` ID. Check freshness and scan
coverage; follow returned continuations for remaining evidence. See the
[graph tool reference](../octocode-native/docs/engine/CODE_GRAPH.md#agent-graph-tools-cli-and-beta-mcp)
for operation requirements, defaults and snapshot retention.

### Setup and management

| Command | Does |
|---|---|
| `schema [tool]` | List all CLI tools with agent instructions, or print one tool's input contract. |
| `config` | Show config and `.env` paths and loaded key names; `home`, `get`, `set`, `unset`, `check`, `view`. |
| `auth status\|login\|logout` | GitHub sign-in with the device flow, its status, and sign-out. |
| `install --ide <id>` | Add (or `--remove`) the Octocode MCP server in a client, at `--scope home\|workspace\|local`; `--list` shows each client's status. |
| `skill list\|info\|install\|remove\|check` | Manage the bundled Agent Skills. |
| `help [command]` | Help for any command (also `--help`, `-h`). |

Text mode writes readable evidence to stdout and guidance to stderr. `--json`
returns the structured result. Exit `0` can include remaining pages: follow
`responsePagination.next` before tool-level `next` calls. The
[CLI guide](../../docs/OCTOCODE_CLI.md#exit-codes) owns exit codes and flags.

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
