# Octocode CLI

`octocode` researches code from your terminal: local files, GitHub
repositories, pull requests and history, and package registries. It also sets
up the Octocode MCP server in your agent client and installs Octocode Agent
Skills. Every research tool runs in a native Rust engine.

## Run

Requires Node.js 24 (24.15.0 or later, 24.x). Run every command through `npx`; nothing
to install:

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
stdin (`--input -`). Batch several with `{"queries":[…]}`.
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
| **GitHub** | |
| `ghSearchRepo` | Discover repositories by keywords, owner, or filters. |
| `ghSearchCode` | Find default-branch code or file paths in an owner or repository. |
| `ghStructure` | Browse a known repository's directory tree, branches, and tags. |
| `ghGetFileContent` | Read a known GitHub file or an exact source region. |
| `ghSearchHistory` | Find pull requests, issues, or commits. |
| `ghGetHistoryItem` | Read a known pull request, issue, commit, or comparison. |
| `ghCloneRepo` | Cache a shallow checkout for repeated local analysis (CLI only). |
| **Packages** | |
| `artifactSearch` | Find package versions, dependencies, or upstream release source (npm, PyPI, crates, Maven, NuGet, Go, Packagist, RubyGems). |
| **Classification** | |
| `clasify` | Locate a described target, rank an unread list, or judge supplied state. Needs `OCTOCODE_CLASSIFICATION_API`. |
| **Beta** (set `OCTOCODE_BETA=true`) | |
| `astTopology` | Analyze local dependency and reachability topology. |
| `astRewrite` | Preview or apply guarded structural code rewrites. |

```bash
npx octocode ghSearchHistory '{"operation":"pullRequest","owner":"bgauryy","repo":"octocode","state":"merged"}'
npx octocode artifactSearch '{"ecosystem":"npm","packageName":"zod"}'
echo '{"path":"src/index.ts","ranges":["1-40"]}' | npx octocode localFetch --input -
```

### Code graph

Parse a repository once, then answer structural questions in milliseconds.

| Command | Does |
|---|---|
| `graph ingest [path]` | Build a snapshot in `<workspace>/.octocode/graph` (an unchanged tree is reused). |
| `graph query <op> [ref]` | Answer one question from the latest snapshot. |

Query operations: `stats`, `find`, `node`, `symbols`, `deps`, `dependents`,
`callers`, `callees`, `path`, `walk`, `cycles`, `hubs`, `diagnostics`, `stale`,
`issues`, `impact`.

```bash
npx octocode graph ingest .
npx octocode graph query callers 'src/server.ts#createServer'
npx octocode graph query impact --since main  # blast radius of your branch
```

### Setup and management

| Command | Does |
|---|---|
| `schema` | List every enabled tool with agent instructions. |
| `schema <tool> [--view query\|variants\|full]` | Print one tool's input contract. |
| `config` | Show config and `.env` file paths and loaded key names (not values). |
| `config home` | Print the Octocode home directory. |
| `config get KEY` | Print a key's resolved value; exit 1 when unset. |
| `config set KEY VALUE` | Set a key in the global `.env` (`--stdin` keeps secrets out of shell history). |
| `config unset KEY` | Remove a key from the global `.env`. |
| `config check KEY` | Exit 0 when a key is set, 1 when unset; never prints the value. |
| `config view` | Open the local settings, API keys, and agent configuration page. |
| `auth status` | Show whether GitHub accepts the active token, and where it comes from. |
| `auth login` | Sign in with the GitHub device flow (`--refresh`, `--force`, `--hostname`). |
| `auth logout` | Remove stored GitHub credentials. |
| `install --ide <id>` | Add the Octocode MCP server to a client (`--list` for ids, `--dry-run`, `--check`, `--force`, `--rollback <file.bak>`, `--method npx\|bunx\|pnpm`, `--enable-local true\|false`, `--pass-env`). Without `--ide` on a terminal, pick a client. |
| `skill list` | List bundled skills with install status. |
| `skill info <name>` | Show a skill and its environment readiness. |
| `skill install <name>...\|--all` | Install skills for `--platform pi,cursor,claude,codex,opencode,copilot,gemini,all`. |
| `skill remove <name>...\|--all` | Remove skills and their platform links. |
| `skill check [--fix]` | Verify installs and links; `--fix` repairs them. |
| `help [command]` | Print help for any command (also `--help`, `-h`). |
| `--version`, `-V` | Print the version. |

Maintenance: `cache status|clear` (GitHub response cache) and
`lsp-server list|install|uninstall|clean|status` (language servers for
`lspSearch`).

## Output and exit codes

On a terminal, tools and `graph` print readable text; on a pipe, or with
`--json`, they print one line of JSON. Errors follow the output: JSON on stdout
in JSON mode, text on stderr otherwise. When more data exists, the response
carries a ready-to-run `next` query.

| Exit | Meaning |
|---:|---|
| 0 | Success |
| 1 | Empty result / no matches |
| 2 | Invalid input |
| 3 | Not found |
| 4 | Authentication required |
| 5 | Execution error |
| 6 | Partial result: run the `next` continuation |
| 7 | Rate limited |
| 130 | Interrupted |

## Configuration

Keys resolve from the process environment, then a workspace `.octocode/.env`,
then the global `.env` in the Octocode home (`npx octocode config home`).

| Key | Purpose |
|---|---|
| `GH_TOKEN` / `GITHUB_TOKEN` | GitHub token; overrides `auth login`. |
| `OCTOCODE_CLASSIFICATION_API` | Enables `clasify`. |
| `OCTOCODE_BETA` | Enables `astTopology` and `astRewrite`. |
| `WORKSPACE_ROOT` | Root that relative local paths resolve against (default: the current directory). |

## Documentation

- [CLI reference](https://github.com/bgauryy/octocode/blob/main/packages/octocode/docs/OCTOCODE_CLI.md):
  every flag, recipes, and graph details
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
