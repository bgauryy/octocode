# octocode-native

> **Package name:** `@octocodeai/octocode-native`  
> **Directory:** `packages/octocode-native/`

Native Rust CLI for Octocode research tools. Runs the same tool engine used by
the MCP server and the Node CLI — local file search, AST analysis, LSP
semantics, GitHub, and package lookup — as a standalone binary with no Node
dependency.

```sh
$ octocode --version
octocode 0.1.0

$ octocode search "ToolRuntime" src/
{"results":[{"data":{"path":"src/runtime/engine.rs",…}}]}

$ octocode files . --names '*.rs' --pretty
{
  "results": [ { "data": { "path": "src", "files": [ … ] } } ]
}
```

## Install

### Via npm (recommended — pre-built binary, no compilation)

```sh
npm install -g @octocodeai/octocode-native
# or
npx @octocodeai/octocode-native --version
```

npm auto-selects the right platform binary via `optionalDependencies`
(`cpu` + `os` guards). Supported platforms:

| Platform package | Target |
|---|---|
| `@octocodeai/octocode-native-darwin-arm64` | macOS Apple Silicon |
| `@octocodeai/octocode-native-darwin-x64` | macOS Intel |
| `@octocodeai/octocode-native-linux-x64-gnu` | Linux x64 (glibc) |
| `@octocodeai/octocode-native-linux-x64-musl` | Linux x64 musl (Alpine, Docker) |
| `@octocodeai/octocode-native-linux-arm64-gnu` | Linux ARM64 (glibc) |
| `@octocodeai/octocode-native-win32-x64-msvc` | Windows x64 |

On Alpine / musl Linux the shim detects `/etc/alpine-release` and resolves
`linux-x64-musl` automatically.

### From source (requires Rust)

```sh
# dev build
yarn workspace @octocodeai/octocode-native build:dev
# or
cargo build --manifest-path packages/octocode-native/Cargo.toml --bins

# release build (LTO + strip, ~50 MB)
cargo build --manifest-path packages/octocode-native/Cargo.toml --bins --release
```

Binary locations:
```sh
packages/octocode-native/target/debug/octocode
packages/octocode-native/target/release/octocode
```

## Architecture

```
main.rs  (8 lines)
  └─ cli::run(Args::parse())
       └─ ToolRuntime::from_host()
       └─ dispatch(command, &rt)
            ├─ human.rs       — research commands (files, tree, symbols, ast, …)
            ├─ search.rs      — lexical/regex search with 30+ flags
            └─ mcp_install.rs — IDE MCP install
```

The `ToolRuntime` carries its own concurrency controller, config loader,
security registry, response shaper, and GitHub cache. Every tool call goes
through the same execution path used by the MCP server and the Node CLI;
`octocode-native` is a different _interface_ to the same engine, not a
separate implementation.

### npm / platform distribution layout

```
packages/octocode-native/
├─ bin/
│   ├─ octocode.cjs              ← platform-selecting Node shim
│   └─ octocode-regex-worker.cjs
├─ npm/
│   ├─ verify-binary.cjs         ← prepublishOnly gate
│   ├─ darwin-arm64/  octocode + octocode-regex-worker
│   ├─ darwin-x64/    octocode + octocode-regex-worker
│   ├─ linux-arm64-gnu/ …
│   ├─ linux-x64-gnu/   …
│   ├─ linux-x64-musl/  …
│   └─ win32-x64-msvc/ octocode.exe + octocode-regex-worker.exe
└─ scripts/
    ├─ copy-binaries.cjs         ← cargo output → npm/<platform>/
    └─ check-platform-binaries.cjs
```

Build a single platform and copy binaries:
```sh
yarn workspace @octocodeai/octocode-native build:darwin-arm64
# → cargo build --release --target aarch64-apple-darwin
# → copies binaries into npm/darwin-arm64/
```

Build all platforms (requires cross-compilation toolchains / CI):
```sh
yarn workspace @octocodeai/octocode-native build:all
```

Check all platform binaries are present before publishing:
```sh
yarn workspace @octocodeai/octocode-native platforms:check
```

## Quick examples

```sh
# local file read (paginated; exit 6 + token on multi-page files)
octocode read src/cli/mod.rs --lines 1:50

# continue a paginated read
octocode next <TOKEN>

# search for a literal string
octocode search "ToolRuntime" src/

# regex search
octocode search 'pub (async )?fn' src/ --regex

# read a remote GitHub file (no clone required)
octocode fetch cli/cli/README.md
octocode fetch cli/cli/pkg/cmd/root.go@main --lines 1:30
octocode fetch https://github.com/anthropics/anthropic-sdk-python/README.md

# list Rust files
octocode files . --names '*.rs'

# filesystem tree
octocode tree src/

# parse + syntax tree for one file
octocode tree src/main.rs --syntax

# declarations in a file
octocode symbols src/cli/mod.rs --name dispatch

# structural AST match
octocode ast src/ 'pub async fn $NAME'

# file-dependency graph
octocode graph . dependencies --file src/cli/mod.rs

# dead-code analysis
octocode graph . deadCode

# LSP — go to definition
octocode def src/cli/human.rs --symbol has_auth_token --line 344

# GitHub repository search
octocode repos "ast-grep pattern matching" --language rust

# GitHub PR search
octocode history prs --repo octocodeai/octocode --keywords "fix"

# read a single PR
octocode history pr --repo octocodeai/octocode --number 42

# read a single commit
octocode history commit --repo octocodeai/octocode --ref abc1234

# package lookup
octocode package clap --ecosystem crates --info

# pretty-print JSON output (any research command)
octocode symbols src/ --pretty

# call a tool directly with raw JSON
octocode tools localSearch '{"queries":[{"searchText":"fn run","path":"src/"}]}'

# print a tool's input schema
octocode tools astSearch --scheme
```

## Commands

### Research — local

| Command | Alias | What it does |
|---|---|---|
| `search <text> <path>` | `s` | Lexical / regex search with ripgrep. 30+ flags — see `search --help`. |
| `read <path>` | | Read a local file; paginates with exit 6 + `next` token. |
| `next <token>` | | Continue a paginated `read`. |
| `files <path>` | | List files; filter with `--names '*.rs,*.ts'`. |
| `tree <path>` | | Filesystem tree. Add `--syntax` for the parsed syntax tree (single file). |
| `symbols <path>` | | Declarations — functions, classes, types, etc. Filter with `--name`. |
| `ast <path> <pattern>` | | Structural AST pattern match (ast-grep syntax). |
| `graph <path> <analysis>` | | File-topology analysis. `analysis`: `deadCode` \| `cycles` \| `dependencies` \| `dependents` \| `path` \| `reachability`. Use `--file` / `--target` for directed queries. |
| `rewrite <path> <pattern> --to <template>` | | Preview or apply a structural rewrite. `--lang` for extension-less paths. `--apply` to write. |

### Research — LSP

All LSP commands accept: `--symbol <name>` `--line <n>` `--character <n>` `--operation <op>` `--pretty`.

| Command | Default LSP operation |
|---|---|
| `def <uri>` | `definition` |
| `refs <uri>` | `references` |
| `callers <uri>` | `callers` |
| `callees <uri>` | `callees` |
| `type <uri>` | `typeDefinition` |
| `diagnostics <uri>` | `diagnostic` |

### Research — GitHub

| Command | What it does |
|---|---|
| `fetch <owner/repo[/path][@branch]>` | Read a file from GitHub without cloning. Accepts `owner/repo/path`, `owner/repo/path@branch`, or a full GitHub URL. |
| `repos <query>` | Search GitHub repositories. `--owner`, `--language`, `--stars`, `--sort`. |
| `clone <owner/repo>` | Clone into the local cache. `--branch`, `--sparse-path`. |
| `history <op> --repo <owner/repo>` | `op`: `prs` \| `issues` \| `commits` (search) or `pr` \| `issue` (requires `--number`) \| `commit` (requires `--ref <SHA>`). |

### Research — packages

| Command | What it does |
|---|---|
| `package <query>` | Keyword discovery or exact lookup (`--info`). `--ecosystem`: `npm` \| `pypi` \| `crates` \| `maven` \| `nuget` \| `go` \| `packagist` \| `rubygems`. |

### Raw tool access

```sh
octocode tools                          # list enabled tools
octocode tools <name> --scheme          # print the tool's input schema
octocode tools <name> '<json>'          # run with a raw JSON query
octocode tools <name> --json --compact  # schema in compact JSON
```

### Workspace / utility

| Command | What it does |
|---|---|
| `context` | Show the active tool context / project manifest. `--full` for extended detail. |
| `status` | Runtime status — config, available tools, auth. `--json`. |
| `config` | Inspect configuration. `--keys` to list key names; `--check <key>` to test. |
| `auth` | Auth status. `--json`. |
| `login` | Authenticate (opens browser or reads token). `--refresh` to force re-auth. |
| `logout` | Remove stored credentials. |
| `cache <action>` | `action`: `status` \| `clear`. Inspect or purge the native GitHub cache. |
| `tools [tool]` | Introspect or call a tool. `--scheme` for the input schema. |
| `install` | Install the MCP server into an IDE. `--ide cursor\|windsurf\|claude\|…`. |
| `skill <args…>` | Pass-through to the `octocode skill` Node CLI. |

### Output flags (research commands)

| Flag | Effect |
|---|---|
| *(default)* | Compact single-line JSON |
| `--pretty` | Indented multi-line JSON |

`--pretty` is available on: `files`, `tree`, `symbols`, `ast`, `graph`, `rewrite`,
all LSP commands, `repos`, `clone`, `package`, `history`, `fetch`.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Empty result / no matches |
| `2` | Invalid input (also clap argument errors) |
| `3` | Not found |
| `4` | Auth required |
| `5` | Execution error |
| `6` | Partial result — continuation token printed to stderr (`octocode next <token>`) |
| `7` | Rate limited |
| `130` | Interrupted (Ctrl-C) |

## octocode-native vs Node CLI

| | `octocode-native` (this package) | `octocode` (Node CLI) |
|---|---|---|
| **Runtime** | Native binary, no runtime required | Requires Node ≥ 24 + npm |
| **Install** | `npm i -g @octocodeai/octocode-native` | `npm i -g octocode` |
| **Startup (help/auth)** | ~8 ms | ~120 ms |
| **Startup (tool call)** | ~160 ms | ~330 ms |
| **Binary size** | ~50 MB release | 1 KB entry + node_modules |
| **Schema-driven flags** | Hardcoded per-command `#[arg]` fields | Auto-derived from live JSON schema |
| **Human output** | Compact JSON or `--pretty` | `--yaml` / `--text` / `--json` / `--compact` |
| **Environments without Node** | ✓ Standalone | ✗ Node required |
| **Interactive UI** | Plain text | Menus, spinners, colored headers |
| **Auth flow** | Device flow (proxies to Node skill) | Native interactive OAuth with keychain |

**Use `octocode-native`** for shell scripts, CI pipelines, environments without
Node, fast config/status checks, and search with full ripgrep flags.

**Use the Node CLI** for interactive IDE install, schema-aware flag derivation,
`--yaml` human output, full OAuth flow, and MCP-compatible structured errors.

## Test

```sh
# all tests
cargo test --manifest-path packages/octocode-native/Cargo.toml

# CLI integration tests only
cargo test --manifest-path packages/octocode-native/Cargo.toml --test cli

# via yarn
yarn workspace @octocodeai/octocode-native test
```

## Key constraints

- **No NAPI in the CLI binary.** NAPI is only compiled with `--features napi-addon` on the lib target.
- **No Node fallback.** The binary terminates with an error rather than shelling out to Node (except `skill` and `login`, which proxy to the Node CLI by design).
- **Strict clippy.** `unwrap_used = deny`, `dbg_macro = deny`.
- **clap v4 derive.** All argument parsing uses `#[derive(Parser)]` / `#[derive(Args)]` — no builder API.
- **Parse-time validation.** All enum-like string args use `value_parser` so bad values exit 2 with choices printed — never a runtime panic.
