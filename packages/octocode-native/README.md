# octocode-native

> **Package name:** `@octocodeai/octocode-native` (private workspace; publishes the `npm/<platform>` packages)  
> **Directory:** `packages/octocode-native/`

The Rust engine behind every Octocode tool, shipped as one npm package per platform:
the standalone `octocode` binary (no Node needed). It runs the CLI commands and the stdio
MCP server (`octocode mcp`), which both protocol eras (2025-11-25 and 2026-07-28) reach
from the same process.

```sh
npx -y octocode --version
npx -y octocode schema
npx -y octocode schema localSearch --view variants
```

Every available tool is a command under its canonical name. CLI and MCP accept the
same JSON envelope: `{ "queries": [...] }` with optional `responseChars`, or an
issued `{ "cursor": "…" }`. Supply it directly or through `--input FILE|-`.
The binary serves `schema` and each tool's `--help` from discovery content that
`@octocodeai/config` projects from core at build time. The `octocode` npm launcher
delegates to it.

## Install

### npm (recommended)

```sh
npx octocode --version        # the CLI
npx octocode-mcp              # the stdio MCP server
```

Both launchers pin one package per platform in `optionalDependencies`, and npm
installs only the one whose `os`, `cpu` and `libc` match the machine:

| Platform package | Target |
|---|---|
| `@octocodeai/octocode-native-darwin-arm64` | macOS Apple Silicon |
| `@octocodeai/octocode-native-darwin-x64` | macOS Intel |
| `@octocodeai/octocode-native-linux-x64-gnu` | Linux x64 (glibc) |
| `@octocodeai/octocode-native-linux-x64-musl` | Linux x64 musl (Alpine) |
| `@octocodeai/octocode-native-linux-arm64-gnu` | Linux ARM64 (glibc) |
| `@octocodeai/octocode-native-linux-arm64-musl` | Linux ARM64 musl (Alpine) |
| `@octocodeai/octocode-native-win32-x64-msvc` | Windows x64 |
| `@octocodeai/octocode-native-win32-arm64-msvc` | Windows ARM64 |

On Linux the launcher picks the musl package when `/usr/bin/ldd` names musl and
the glibc one when it carries the GNU C Library header; only an `ldd` that names
neither falls back to Node's glibc report and `ldd --version`.

An installed `octocode` command is already the binary when its postinstall ran
([launcher architecture](../octocode/ARCHITECTURE.md#runtime-boundary)). Other
scripts that call the binary many times can skip the launcher's Node startup by
running the binary it would start: `resolveNativeBinaryPath()` from
`bin/resolve-binary.cjs` is the launcher's own resolver and returns that path
(it throws when the platform package is missing; the launcher then downloads
it). Keep the launcher when `OCTOCODE_NATIVE_BIN` is set, since it applies that
override's policy.

When npm skipped the platform package (`--omit=optional`, or a lockfile written
on another platform), the launcher downloads that package once at the pinned
version from the configured npm registry, checks the registry's sha512 integrity
and that the binary reports the expected version, and keeps it under
`~/.octocode/native/<version>/<platform>/` (`OCTOCODE_HOME` moves it). It runs
at first launch rather than in an install script, so it also works where
install scripts are disabled (pnpm 10, `--ignore-scripts`).

`bin/platform.cjs` (`PLATFORMS`) is the one platform list.
`node scripts/platform-packages.cjs` writes every `npm/<platform>/` manifest and
README and both launchers' pins from it; `--check` (part of `test:scripts`)
fails when one drifts.

### Homebrew (not available yet)

The `bgauryy/octocode` tap still ships the old Node-based 2.0.0 formula, not this
binary. Use npm until the native formula is published.

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

Seventeen internal crates (`publish = false`, one workspace version). The last column
lists every Cargo dependency between them.

| Crate | Owns | Depends on |
|---|---|---|
| `crates/io` | Cross-crate primitives: owner-only atomic writes, `flock` waits, environment reads, the HTTP client policy, lock-free request cancellation, the in-process memo, keyed single flight, stderr notices ([`lib.rs`](crates/io/src/lib.rs)) | none |
| `crates/contract-base` | The generated wire types the crates below `contract-types` name (`include!`s `packages/octocode-config/contract/tool_types_base.rs`) | none |
| `crates/contract-types` | The tool wire types generated from the contract (`include!`s `packages/octocode-config/contract/tool_types.rs`), re-exporting `contract-base` | `contract-base` |
| `crates/engine` | Reusable algorithms: search, AST, signatures, graph facts, minification, the language-server pool | `io` |
| `crates/github` | GitHub protocol and transport, retries, `Partition` ([`lib.rs`](crates/github/src/lib.rs)) | `io`, `contract-base` |
| `crates/runtime-base` (`octocode-runtime-base`) | The layers below the tools: config, credentials, contracts, availability, policy, security, responses, providers; the tools' shared test fixtures (`test_support`, feature `test-support`) | `io`, `contract-types`, `engine`, `github` |
| `crates/tools-shared` (`octocode-tools-shared`) | Logic two or more tools use, in layers (`SHARED_LAYERS`): walks, bounded processes, judgment, selection, leads, the source view and capture; the `selection_fake` and `read_view` fixtures behind `test-support` | `io`, `contract-types`, `engine`, `github`, `runtime-base` |
| `crates/tool-ast` (`octocode-tool-ast`) | `astSearch` (`ast_search`) | `io`, `contract-types`, `engine`, `runtime-base`, `tools-shared` |
| `crates/tool-clasify` (`octocode-tool-clasify`) | `clasify` | `io`, `contract-types`, `engine`, `github`, `runtime-base`, `tools-shared` |
| `crates/tool-graph` (`octocode-tool-graph`) | `graphBuild` and `graphQuery` (`ast_graph`) and the persisted code graph | `io`, `contract-types`, `engine`, `runtime-base`, `tools-shared` |
| `crates/tool-local` (`octocode-tool-local`) | `localSearch`, `structureSearch` and `localFetch` (`local_search`, `structure_search`, `local_fetch`) | `io`, `contract-types`, `engine`, `runtime-base`, `tools-shared` |
| `crates/tool-lsp` (`octocode-tool-lsp`) | `lspSearch` (`lsp_search`) | `io`, `contract-types`, `engine`, `runtime-base`, `tools-shared` |
| `crates/tool-packages` (`octocode-tool-packages`) | `securityCheck` and `artifactSearch`, with the package-registry providers they read (`artifact`) | `io`, `contract-types`, `engine`, `github`, `runtime-base`, `tools-shared` |
| `crates/test-support` | Test fixtures no release build links: the wiremock mounts (`mock_http`) and the engine's conformance grammar; a dev-dependency only | `engine` |
| `crates/tool-github` (`octocode-tool-github`) | The GitHub tools: `ghSearchCode`, `ghSearchRepo`, `ghSearchHistory`, `ghGetHistoryItem`, `ghGetFileContent`, `ghStructure`, `ghCloneRepo` (`gh_*`) | `io`, `contract-types`, `engine`, `github`, `runtime-base`, `tools-shared`, `test-support` |
| `crates/runtime` (`octocode-native`) | `ToolRuntime`: admission, dispatch, the tool catalog and the build identity; the seam-crossing tests | `io`, `contract-types`, `engine`, `github`, `runtime-base`, `tools-shared`, `tool-ast`, `tool-clasify`, `tool-github`, `tool-graph`, `tool-local`, `tool-lsp`, `tool-packages` |
| `crates/cli` | The `octocode` binary: its commands, its warm tool server and the stdio MCP server (`octocode mcp`) | `io`, `contract-types`, `engine`, `github`, `runtime-base`, `runtime` |

Design and invariants: [ARCHITECTURE.md](ARCHITECTURE.md). Languages and what
each feature supports:
[SUPPORTED_LANGUAGES_AND_FEATURES.md](docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md).
Language servers: [LSP_SERVER_LIFECYCLE.md](docs/engine/LSP_SERVER_LIFECYCLE.md).
Persisted code graph: [CODE_GRAPH.md](docs/engine/CODE_GRAPH.md). Direct
dependencies: [DEPENDENCY_AUDIT.md](docs/DEPENDENCY_AUDIT.md).

```
packages/octocode-native/
├─ crates/{engine,io,github,contract-base,contract-types,runtime-base,runtime,cli}/
├─ bin/         platform map, binary resolver, registry fallback, signal-forwarding launcher (bundled into the launchers)
├─ npm/<p>/     one package per platform: the `octocode` binary
└─ scripts/     build-native.cjs (build + atomic staging), stage-binary.cjs, platform-packages.cjs (derived manifests),
               release: binary-identity.cjs, pack-release.cjs (docs/PUBLISHING.md)
```

Cross-platform and release builds (`build:target <platform>`, `build:all`) and
publishing: [PUBLISHING.md](docs/PUBLISHING.md).

## Examples

```sh
# availability and agent instructions, then one tool's query fields
npx octocode schema
npx octocode schema ghSearchHistory --view query --select operation=commit

# local tools (paths relative to packages/octocode-native)
npx octocode localFetch '{"queries":[{"path":"crates/cli/src/main.rs","ranges":[{"startLine":1,"endLine":50}]}]}'
npx octocode localSearch '{"queries":[{"matchString":["ToolRuntime"],"path":"crates/","view":"content"}]}'
npx octocode astSearch '{"queries":[{"operation":"match","path":"crates/","parser":"rust","rule":{"pattern":"async fn $NAME($$$ARGS) -> $T { $$$BODY }"}}]}'
npx octocode localSearch '{"queries":[{"path":"crates/cli/src/cli.rs","matchString":["dispatch(command"]}]}'
# For a definition lookup, pass a returned path and line with symbolName "dispatch" to lspSearch.

# GitHub and packages
npx octocode ghGetFileContent '{"queries":[{"owner":"cli","repo":"cli","path":"README.md"}]}'
npx octocode ghSearchHistory '{"queries":[{"operation":"pullRequest","owner":"bgauryy","repo":"octocode","keywords":["fix"]}]}'
npx octocode artifactSearch '{"queries":[{"operation":"resolve","ecosystem":"crates","packageName":"clap"}]}'

# a long query from a file or stdin
npx octocode localSearch --input query.json
```

A paged result exits `0`. Copy the `input` from its `next` call to the named tool.
A call is `{tool, input}`: the reply's top-level `next` is its one required
continuation (`input:{cursor}`), a row's `next.*` is a branch; under `related`
it starts optional work. `hints` are prose.
Field reference: [OCTOCODE_TOOLS.md](../../docs/OCTOCODE_TOOLS.md).

## Commands

Tool commands take one JSON envelope (positional, `--input <file>`, or `--input -`). They print text on a terminal or a pipe:
rg-style lines for search hits, the rendered result otherwise, each row followed on
stdout by its status, page state, warnings, hints and runnable continuation commands as `# ` lines;
counters and `related[]` leads are `# ` notes on stderr (`crates/cli/src/cli/lines.rs`), and so is progress when
stderr is a terminal. A call
rejected before it runs prints on stdout as a failed row does. `--json`
prints JSON, indented on a terminal. `octocode schema` lists the CLI tools available with the current configuration. Graph, clone and advisory tools are opt-ins on MCP. `clasify` requires a provider key on both interfaces. See the [availability matrix](../../docs/CONFIGURATION.md#tool-availability-cli-and-mcp).

| Command | What it does |
|---|---|
| `schema [tool]` | Tool catalog with agent instructions, or one tool's contract (`--view query`, `--view variants`, `--select operation=VALUE[,FIELD=VALUE]`) |
| `config` | Open the local settings page on an interactive terminal with a display. Use `--json` for paths, loaded key names and warnings. Subcommands include `list`, `view`, `home`, `get KEY`, `set KEY VALUE` / `set KEY --stdin`, `unset KEY`, `check KEY` |
| `cache clear` | Drain warm CLI servers and remove tool caches; [scope and MCP restart](../../docs/OCTOCODE_CLI.md#clear-tool-caches) |
| `auth` | `status`, `login` (device flow; `--hostname`, `--force`, `--refresh`), `logout` |
| `graphBuild`, `graphQuery` | Build and query a persisted code graph ([CODE_GRAPH.md](docs/engine/CODE_GRAPH.md)) |
| `install` | Add the MCP server to an agent client: `--ide <id>` from `--list` |
| `skill list\|info\|install\|remove\|check` | Manage the embedded public skills without Node |
| `help` | Help for the binary or a command |

Hidden maintenance commands: `cache clear`, `lsp-server <list|install|uninstall|clean|status>`, and `serve` (the warm
tool server, started automatically). Exit codes are printed by
`octocode --help` and documented in the
[CLI guide](../../docs/OCTOCODE_CLI.md#exit-codes).

### Warm tool server

Every read-only tool call (all tools except `ghCloneRepo` and `graphBuild`, which write)
runs on `octocode serve`, so a later page reuses what an earlier call computed,
as it does in one MCP session: issued pages, stored scans and continuation-selected symbol evidence, alongside
live language servers. A fresh request reuses only what is keyed by the
content it reads (outlines, parsed declarations); anything else it computes again. The first call of a workspace
starts the server, detached, behind a socket in `<temp dir>/octocode-<uid>`
(a 0700 directory only the owner can enter). Both interfaces reach it: CLI calls,
and the read-only tools of an MCP session (`octocode mcp`), so a workspace has one
language-server pool, one memory budget and one set of warm indexes whichever
client asks.

- **One server per** binary build, workspace (the nearest `.git` ancestor of
  the working directory, else the directory) and process environment. The
  build is the executable file's device and inode, its version, size and
  modification time, with no path: a symlink, a relative spelling or a hard
  link is the same build, a moved binary keeps its server, and a copy is
  another build with its own server (copying a binary gives it new servers; link
  it instead). The process environment is every variable the engine reads
  from the server's own process (`PROCESS_ENV`: `HOME` and `USERPROFILE`) and
  the proxy and trust-store variables (`NETWORK_ENV`). A call
  cannot bring its own value of these, so another value reaches another
  server, and a server answers `fallback` to a request made under a process
  environment other than its own. The socket name is a hash (with
  `KEY_VERSION`, bumped when the key's meaning changes, so a newer client
  never meets an older server), so it holds no secret.
- **Call environment.** `PATH`, `LANG`, `TMPDIR`, the rustup, Go, Python,
  JVM and .NET toolchain variables (`CALL_ENV`) travel with each call and are
  read only from it: language-server discovery and start run under the
  caller's, so a GUI-launched MCP server and a terminal's CLI share one
  server whatever their `PATH`. The language server's own environment is
  part of the pool key, so callers whose environments differ in what a
  server is given (`PATH`, `LANG`, ...) get one language server each; equal
  environments share one. (`TMPDIR` also names the socket directory, so
  clients with another `TMPDIR` reach another server.)
- **Configuration keys the runtime, not the server.** The resolved
  configuration (home and workspace `.env` and `.octocoderc`, every variable the
  config contract declares, the environment GitHub credential's source and
  digest) is the identity a runtime is kept under; a call under another
  configuration, or one whose files changed, gets its own runtime on the same server. A
  variable no setting declares keys nothing.
- **Per directory, surface and environment.** A call runs as a CLI (or MCP
  server) started in its own directory with its own environment would: the
  request carries the environment, so credential discovery (`gh` and
  `GH_CONFIG_DIR`, npm config) sees the caller's. The server keeps a runtime per
  directory, surface and environment (up to `MAX_RUNTIMES`), on one shared
  language-server pool, so directories of one repository, and an MCP session
  beside CLI calls, share one rust-analyzer. The surface decides what the
  runtime enforces: an MCP call keeps MCP's tool lists, beta gating and
  prerequisites (`tools.disabled`, `tools.enabled`) while the CLI ignores them, and
  answers as an MCP result: the same bytes the in-process MCP server sends.
- **Credentials.** Each request reloads `gh` or stored credentials, so account
  switching, logout and token rotation reach the next request. `octocode auth
  login` and `logout` also stop every warm server.
- **Same output.** Stdout, the exit code, and stderr (configuration warnings,
  and the warnings a call raises, such as an unusable credential store or
  rate-limit state file) match an in-process call; an MCP call's text is
  byte-identical to the in-process server's. Ctrl-C cancels the call on
  the server, and so does the MCP client's `notifications/cancelled` (the MCP
  process closes its side of the connection). An MCP call's progress arrives
  as `notifications/progress` from the server's progress frames, and
  `lsp.prewarm` `all` asks the server to warm the session's workspace.
- **In process instead** with `storage.mode = memory`
  (`OCTOCODE_STORAGE_MODE=memory`), on Windows (CLI and MCP alike), or for the tools that write (`ghCloneRepo`, `graphBuild`). A server that cannot be reached, refuses a call, or fails
  mid-call (it died, or sent nothing for 300 s) is named on stderr and the call
  runs in process, once, with no second try on a fresh server. An MCP session whose
  server cannot start runs in process from then on. The client waits
  for the reply as long as the server's request budget allows; the CLI sets
  none, so it waits until the reply, the server closing, or Ctrl-C, as long as
  the server keeps talking: while a call runs the server sends a heartbeat
  every 30 s, and a server that sends nothing for 300 s (stopped or hung) is a
  failed server, so the call runs in process. With a budget, a server that
  sends no reply within it answers `timeout`. A client that stops reading
  never stalls its call; it gets the newest progress lines, and one that
  takes no byte for 300 s is treated as gone, which cancels its call.
- **Bounded by demand.** There is no cap on servers per user, and a starting
  server never stops another: a server exits after `IDLE_EXIT` without a
  request (a connection that sends none does not count), and its language
  servers stop earlier on the pool's idle timeout and memory budget. A server
  asked to stop (`auth login`/`logout`, cache clear) finishes the calls it
  has; a connection that has sent no request neither keeps it alive nor
  delays its stop. `cache clear` waits for that drain as long as it lasts,
  hearing a heartbeat every 30 s; a server that sends nothing for 300 s ends
  the wait with an error, and the cache is left uncleared.

The named limits are constants in
[`crates/cli/src/cli/serve/unix/daemon.rs`](crates/cli/src/cli/serve/unix/daemon.rs) (`IDLE_EXIT`, 10 minutes; `MAX_RUNTIMES`), [`surface.rs`](crates/cli/src/cli/serve/unix/surface.rs) (the two surfaces) and
[`frames.rs`](crates/cli/src/cli/serve/unix/frames.rs) (`HEARTBEAT`, `BACKLOG`), and `READ_STALL` in
[`crates/io/src/http.rs`](crates/io/src/http.rs); the
[`serve.rs`](crates/cli/src/cli/serve.rs) module documentation says how the server keeps these guarantees.

`npx octocode` (the Node launcher) runs this binary with unchanged arguments and
inherited stdio.
Native owns the interactive `install` picker, skill commands, and `config view`.

## Test

```sh
yarn workspace @octocodeai/octocode-native test:rust   # workspace Rust tests: libtest, and nextest for the child-process tests
yarn workspace @octocodeai/octocode-native test        # script checks, Node tests, then test:rust
yarn workspace @octocodeai/octocode-native verify      # fmt, oxlint + clippy, cargo deny, cargo shear, tsgo typecheck, tests, contract freshness
cargo dev-test -- <filter>                             # the test:rust selection under libtest, reusing its build
```

`cargo dev-test` (an alias in `.cargo/config.toml`) is the local loop: it
carries the `test:rust` packages and features, so it reuses that build instead
of compiling another copy, and libtest runs each test binary in one process.
`test:rust`, which every gate uses, runs the same selection under libtest
except the tests that start real child processes (`PROCESS_TESTS`): nextest runs
those beside it, one process per test, and kills one stuck past 3 minutes.
`scripts/rust-tests.cjs` holds the selections and says why. Under libtest a stuck
test shows as libtest's `has been running for over 60 seconds`, then
`scripts/test-watchdog.cjs` fails the run by the test's name 10 minutes later
(`OCTOCODE_TEST_STUCK_SECONDS`). `--test-threads=1` prints no such line, so nothing
guards that run.

## Constraints

- One binary serves CLI and MCP; the MCP interface adds no tool logic (`crates/cli/src/cli/mcp_server/` lists tools from the embedded presentation and calls the shared runtime).
- No command delegates back to Node; the CLI owns all command behavior.
- Workspace clippy policy (root `Cargo.toml`, settings in `clippy.toml`) denies `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`, and `dbg_macro`; hand-written code suppresses a lint only with `#[expect(..., reason = "...")]`; a source module is `name.rs` plus `name/` (`mod_module_files`).
- Arguments use clap v4 derive; enum-like arguments are validated at parse time, so a bad value exits `2` with the choices listed.
