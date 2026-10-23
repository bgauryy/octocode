# octocode-native

> **Package name:** `@octocodeai/octocode-native`  
> **Directory:** `packages/octocode-native/`

The Rust engine behind every Octocode tool, shipped as one npm distribution: the
standalone `octocode` binary (no Node needed). It runs the CLI commands and the stdio
MCP server (`octocode mcp`), which both protocol eras (2025-11-25 and 2026-07-28) reach
from the same process.

```sh
$ npx octocode --version
octocode 20.2.0

$ npx octocode schema
{"kind":"octocode.toolCatalog","version":1,"toolCount":13,"commands":{…},"instructions":"…","grammars":"…","tools":[…]}

$ npx octocode schema localSearch --view query
{"name":"localSearch","description":"…","querySchema":{…},"availability":{"enabled":true},"run":"octocode localSearch '<json>'"}

$ npx octocode localSearch ToolRuntime crates/ -l
crates/cli/src/cli/config.rs
crates/cli/src/cli/graph.rs
…
```

Every available tool is a command under its canonical name, with the same query
contract as the MCP server. Its flags are generated from that contract: one long
flag per field, plus short letters rg, grep, fd, ast-grep and gh use for the same
meaning (`crates/cli/src/cli/flags.rs`); one JSON query works too. The
binary serves `schema` and each tool's `--help` itself, from discovery content
that `@octocodeai/config` projects from the core contract at build time, so an
agent with only the binary can discover every tool. The `octocode` npm launcher
delegates to it.

## Install

### npm (recommended)

```sh
npx @octocodeai/octocode-native --version
```

npm installs only the matching platform package through `optionalDependencies`
(`os`/`cpu` guards):

| Platform package | Target |
|---|---|
| `@octocodeai/octocode-native-darwin-arm64` | macOS Apple Silicon |
| `@octocodeai/octocode-native-darwin-x64` | macOS Intel |
| `@octocodeai/octocode-native-linux-x64-gnu` | Linux x64 (glibc) |
| `@octocodeai/octocode-native-linux-x64-musl` | Linux x64 musl (Alpine) |
| `@octocodeai/octocode-native-linux-arm64-gnu` | Linux ARM64 (glibc) |
| `@octocodeai/octocode-native-win32-x64-msvc` | Windows x64 |
| `@octocodeai/octocode-native-win32-arm64-msvc` | Windows ARM64 |

On Linux x64 the launcher picks `linux-x64-musl` when `/usr/bin/ldd` names musl,
or when Node reports no glibc runtime and `ldd --version` names musl. musl on
ARM64 is unsupported.

### Homebrew (not available yet)

The `bgauryy/octocode` tap still ships the old Node-based 2.0.0 formula, not this
binary. Use npm until the native formula is published
([release guide](../../skills-dev/octocode-dev/docs/RELEASE.md), row 8).

### From source (requires Rust)

```sh
yarn workspace @octocodeai/octocode-native build:dev   # host binary, staged into npm/<host>/
yarn workspace @octocodeai/octocode-native build       # the same, release profile
```

`scripts/build-native.cjs` builds the binary in one Cargo invocation, then stages
it atomically. A plain
`cargo build --manifest-path packages/octocode-native/Cargo.toml` (default member
`crates/cli`) writes `target/debug/octocode` without staging.

## Architecture

Six internal crates (`publish = false`, one workspace version). The last column
lists every Cargo dependency between them.

| Crate | Owns | Depends on |
|---|---|---|
| `crates/engine` | Reusable algorithms: search, AST, signatures, graph facts, minification, the language-server pool | none |
| `crates/io` | Cross-crate primitives: owner-only atomic writes, `flock` waits, environment reads, the HTTP client policy, the in-process memo, keyed single flight, stderr notices ([`lib.rs`](crates/io/src/lib.rs)) | none |
| `crates/github` | GitHub protocol and transport, retries, `Partition` ([`lib.rs`](crates/github/src/lib.rs)) | `io` |
| `crates/contract-types` | The tool wire types generated from the contract (`include!`s `packages/octocode-config/contract/tool_types.rs`) | none |
| `crates/runtime` (`octocode-native`) | `ToolRuntime`: config, credentials, contracts, availability, tools, responses | `io`, `engine`, `github`, `contract-types` |
| `crates/cli` | The `octocode` binary: its commands, its warm tool server and the stdio MCP server (`octocode mcp`) | `io`, `runtime`, `engine`, `github` |

Design and invariants: [ARCHITECTURE.md](ARCHITECTURE.md). Languages and what
each feature supports:
[SUPPORTED_LANGUAGES_AND_FEATURES.md](docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md).
Language servers: [LSP_SERVER_LIFECYCLE.md](docs/engine/LSP_SERVER_LIFECYCLE.md).
Persisted code graph: [CODE_GRAPH.md](docs/engine/CODE_GRAPH.md). Direct
dependencies: [DEPENDENCY_AUDIT.md](docs/DEPENDENCY_AUDIT.md).

```
packages/octocode-native/
├─ crates/{engine,io,github,contract-types,runtime,cli}/
├─ bin/         platform-selecting launcher (`octocode-native` bin)
├─ npm/<p>/     one package per platform: the `octocode` binary
└─ scripts/     build-native.cjs (build + atomic staging), stage-binary.cjs
```

Cross-platform and release builds (`build:target <platform>`, `build:all`) and
publishing: [PUBLISHING.md](docs/PUBLISHING.md).

## Examples

```sh
# availability and agent instructions, then one tool's query fields
npx octocode schema
npx octocode schema ghSearchHistory --view query --select operation=commit

# local tools (paths relative to packages/octocode-native)
npx octocode localFetch '{"queries":[{"path":"crates/cli/src/main.rs","ranges":["1-50"]}]}'
npx octocode localSearch '{"queries":[{"matchString":"ToolRuntime","path":"crates/","resultView":"matchOnly"}]}'
npx octocode astSearch '{"queries":[{"operation":"match","path":"crates/","language":"rust","pattern":"async fn $NAME($$$ARGS) -> $T { $$$BODY }"}]}'
npx octocode lspSearch '{"queries":[{"operation":"definition","path":"crates/cli/src/cli/mod.rs","symbolName":"dispatch","line":341}]}'

# GitHub and packages
npx octocode ghGetFileContent '{"queries":[{"owner":"cli","repo":"cli","path":"README.md"}]}'
npx octocode ghSearchHistory '{"queries":[{"operation":"pullRequest","owner":"bgauryy","repo":"octocode","keywords":["fix"]}]}'
npx octocode artifactSearch '{"queries":[{"ecosystem":"crates","packageName":"clap"}]}'

# a long query from a file or stdin
npx octocode localSearch --input query.json
```

A paged result exits `0` and carries a `next.*` continuation: run it unchanged.
Field reference: [OCTOCODE_TOOLS.md](../../docs/OCTOCODE_TOOLS.md).

## Commands

Tool commands take flags and positional arguments, or one JSON query (positional,
`--input <file>`, or `--input -`). They print text on a terminal or a pipe:
rg-style lines for search hits, the rendered result otherwise, with notes and
runnable follow-up commands on stderr (`crates/cli/src/cli/lines.rs`). `--json`
prints JSON, indented on a terminal. `octocode --help` lists the tools available under the
current configuration: by default the 12 MCP tools plus the CLI-only
`ghCloneRepo`. Beta `clasify` (with a classification key) appears with
`OCTOCODE_BETA=true`. Gates: [OCTOCODE_TOOLS.md](../../docs/OCTOCODE_TOOLS.md)
and [clasify availability](../../docs/OCTOCODE_CLASIFY.md#availability).

| Command | What it does |
|---|---|
| `schema [tool]` | Tool catalog with agent instructions, or one tool's contract (`--view query`, `--view variants`, `--select FIELD=VALUE`) |
| `config` | Config paths, loaded key names, and warnings; never values (`--json`). Subcommands: `view`, `home`, `get KEY`, `set KEY VALUE` / `set KEY --stdin`, `unset KEY`, `check KEY` |
| `auth` | `status`, `login` (device flow; `--hostname`, `--force`, `--refresh`), `logout` |
| `graph` | `ingest <path>` and `query <op>` over a persisted code graph ([CODE_GRAPH.md](docs/engine/CODE_GRAPH.md)) |
| `install` | Add the MCP server to an agent client: `--ide <id>` from `--list` |
| `skill list\|info\|install\|remove\|check` | Manage the embedded public skills without Node |
| `help` | Help for the binary or a command |

Hidden maintenance commands: `cache <status|clear>`,
`lsp-server <list|install|uninstall|clean|status>`, and `serve` (the warm
tool server, started automatically). Exit codes are printed by
`octocode --help` and documented in the
[CLI guide](../../docs/OCTOCODE_CLI.md#exit-codes).

### Warm tool server

Every read-only tool call (all tools except `ghCloneRepo`)
runs on `octocode serve`, so a later page reuses what an earlier call computed,
as it does in one MCP session: page memos, stored scans, outlines, symbol
pages, graph reports, and warm language servers. The first call of a workspace
starts the server, detached, behind a socket in `<temp dir>/octocode-<uid>`
(a 0700 directory only the owner can enter).

- **One server per** binary build, workspace (the nearest `.git` ancestor of
  the working directory, else the directory), and call identity: the resolved
  configuration (home and workspace `.env` and `.octocoderc`, every variable the
  config contract declares), the environment GitHub credential's source and
  digest, every `OCTOCODE_*` variable such as `OCTOCODE_BETA`, the proxy and
  trust-store variables, and the variables the engine reads from the server's
  own process (`PROCESS_ENV`). Any change starts a new server; the socket name
  is a hash, so it holds no secret.
- **Per directory and environment.** A call runs as a CLI started in its own
  directory with its own environment would: the request carries the
  environment, so credential discovery (`gh` and `GH_CONFIG_DIR`, npm config)
  sees the caller's. The server keeps a runtime per directory and environment
  (up to `MAX_RUNTIMES`), on one shared language-server pool, so directories of
  one repository share one rust-analyzer.
- **Credentials.** A remembered `gh` or stored credential expires after the
  resolver's memo TTL, so `gh auth switch`, a logout, or a rotated token reaches
  a warm server within it; `octocode auth login` and `logout` stop every warm
  server.
- **Same output.** Stdout, the exit code, and stderr (configuration warnings,
  and the warnings a call raises, such as an unusable credential store or
  rate-limit state file) match an in-process call. Ctrl-C cancels the call on
  the server.
- **In process instead** with `storage.mode = memory`
  (`OCTOCODE_STORAGE_MODE=memory`), on Windows, or for `ghCloneRepo`. A server that cannot be reached, refuses a call, or fails
  mid-call is named on stderr and the call runs in process; a server that
  sends no reply within the request budget answers `timeout`.
- **Bounded.** At most `MAX_SERVERS` servers per user: a starting server asks
  the least recently used ones past the cap to stop, and a stopping server
  finishes the calls it has. A server exits after `IDLE_EXIT` without a
  request.

The named limits are constants in
[`crates/cli/src/cli/serve/`](crates/cli/src/cli/serve/mod.rs), whose module
documentation says how the server keeps these guarantees.

`npx @octocodeai/octocode-native` runs this binary directly. `npx octocode` (the
Node launcher) runs the same binary with unchanged arguments and inherited stdio.
Native owns the interactive `install` picker, skill commands, and `config view`.

## Test

```sh
yarn workspace @octocodeai/octocode-native test:rust   # every crate with cargo-nextest, including the N-API adapter
yarn workspace @octocodeai/octocode-native test        # script checks, Node tests, then test:rust
yarn workspace @octocodeai/octocode-native verify      # fmt, oxlint + clippy, cargo deny, cargo shear, tsgo typecheck, tests, contract freshness
cargo dev-test -- <filter>                             # the test:rust selection under libtest, reusing its build
```

`cargo dev-test` (an alias in `.cargo/config.toml`) is the local loop: it
carries the `test:rust` packages and features, so it reuses that build instead
of compiling another copy, and libtest runs each test binary in one process.
`test:rust` stays on nextest; `scripts/test-rust-selections.cjs` says why.

## Constraints

- One binary serves CLI and MCP; the MCP interface adds no tool logic (`crates/cli/src/cli/mcp_server/` lists tools from the embedded presentation and calls the shared runtime).
- No command delegates back to Node; the CLI owns all command behavior.
- Workspace clippy policy denies `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`, and `dbg_macro`.
- Arguments use clap v4 derive; enum-like arguments are validated at parse time, so a bad value exits `2` with the choices listed.
