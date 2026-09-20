# octocode-native

> **Package name:** `@octocodeai/octocode-native`  
> **Directory:** `packages/octocode-native/`

Consolidated distribution for the Octocode native CLI, the `NativeRuntime`
Node addon, and reusable engine primitives. The CLI runs without Node; Node
consumers load the runtime from `.` or `./runtime` and primitives from
`./engine`.

```sh
$ octocode --version
octocode 20.0.0

$ octocode scheme
{"kind":"octocode.toolCatalog","instructions":"…","tools":[…]}  # availability + canonical agent workflow

$ octocode scheme localSearch
{"name":"localSearch","instructions":"…","querySchema":{…}}     # workflow + complete tool contract

$ octocode localSearch '{"searchText":"ToolRuntime","path":"src/","resultView":"matchOnly","reasoning":"Locate the runtime entry."}'
{"results":[{"data":{"searchEngine":"rg","files":[…]}}]}
```

Every tool is a first-class command under its canonical name — the same name
and the same JSON query contract as the MCP server. There are no per-tool flag
wrappers and no aliases. Every `scheme` catalog or tool projection includes the
same availability-scoped canonical agent instructions used by the MCP server.
The compact catalog renders each core-owned `shortDescription` as
`tools[].description`; a tool-specific scheme retains both the short and full
descriptions.

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
cargo build --manifest-path packages/octocode-native/Cargo.toml -p octocode-native --bins --no-default-features

# release build (LTO + strip)
cargo build --manifest-path packages/octocode-native/Cargo.toml -p octocode-native --bins --release --no-default-features
```

Binary locations:
```sh
packages/octocode-native/target/debug/octocode
packages/octocode-native/target/release/octocode
```

## Architecture

```text
crates/runtime  ── Rust rlib ──▶ crates/engine
      │                              │
      ├─ native CLI                  └─ primitive N-API addon (`./engine`)
      └─ NativeRuntime addon (`.` / `./runtime`)
```

The runtime crate owns policy, providers, contracts, cancellation, response
shaping, and CLI behavior. The engine crate owns reusable search, syntax,
minification, security, graph, and LSP algorithms. They remain separate crates
and separate addons even though one npm distribution owns their artifacts.

### Language boundary

The default release has 12 first-class source-language families and exactly 30
extensions: JavaScript, TypeScript, Rust, Python, C, C++, CUDA, Assembly, Java,
Scala, Go, and C#. Structural search/rewrite, signatures, graph facts, syntax
inspection, and LSP grammar adapters derive from one registry. Built-in semantic
server routes cover 11 families and 27 extensions: CUDA uses `clangd`, while
generic Assembly requires trusted custom LSP configuration. Text search,
ordinary reads, generic best-effort minification, artifact lookup, and trusted
custom LSP configuration remain language-agnostic. See
[`docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md`](docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md). The direct Rust dependency necessity and footprint receipt is in
[`docs/engine/DEPENDENCY_AUDIT.md`](docs/engine/DEPENDENCY_AUDIT.md).

### npm / platform distribution layout

```
packages/octocode-native/
├─ crates/runtime/               ← ToolRuntime, CLI, runtime N-API
├─ crates/engine/                ← reusable primitives + engine N-API
├─ js/                           ← independent runtime and engine loaders
├─ bin/                          ← platform-selecting CLI launchers
├─ npm/                          ← six packages, each with four artifacts
│   ├─ darwin-arm64/
│   ├─ darwin-x64/
│   ├─ linux-arm64-gnu/
│   ├─ linux-x64-gnu/
│   ├─ linux-x64-musl/
│   └─ win32-x64-msvc/
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
# discover availability + canonical workflow, then one tool's contract
octocode scheme
octocode scheme localFetch
octocode scheme ghSearch --view query --select operation=code   # workflow + one union branch

# local file read (paginated; exit 6 + a re-runnable next.* continuation in the JSON)
octocode localFetch '{"path":"src/cli/mod.rs","startLine":1,"endLine":50,"reasoning":"Read the dispatch entry."}'

# continue a paginated read: re-run results[].data.next.continue.query verbatim
octocode localFetch '{"path":"src/cli/mod.rs","chunkType":"lines","offset":50,"reasoning":"Continue the read."}'

# lexical / regex search
octocode localSearch '{"searchText":"ToolRuntime","path":"src/","resultView":"matchOnly","reasoning":"Locate the runtime entry."}'

# structural AST match
octocode astSearch '{"operation":"match","path":"src/","pattern":"pub async fn $NAME","langType":"rust","reasoning":"List async entry points."}'

# structural rewrite (preview first; apply requires snapshot + expectedHashes from the preview)
octocode astRewrite '{"path":"src/","langType":"rust","ruleKind":"pattern","pattern":"dbg!($X)","rewrite":"$X","reasoning":"Strip debug macros."}'

# LSP — go to definition
octocode lspSearch '{"operation":"definition","uri":"src/cli/mod.rs","symbolName":"dispatch","lineHint":244,"reasoning":"Jump to dispatch."}'

# read a remote GitHub file (no clone required)
octocode ghGetFileContent '{"owner":"cli","repo":"cli","path":"README.md","reasoning":"Read upstream docs."}'

# GitHub repository / code search
octocode ghSearch '{"operation":"repositories","keywords":["ast-grep"],"reasoning":"Find pattern-matching repos."}'

# PR / issue / commit history
octocode ghSearchHistory '{"operation":"pullRequests","owner":"octocodeai","repo":"octocode","keywords":["fix"],"reasoning":"Find fix PRs."}'
octocode ghGetHistoryItem '{"operation":"pullRequest","owner":"octocodeai","repo":"octocode","number":42,"reasoning":"Read PR 42."}'

# package lookup
octocode artifactSearch '{"type":"crates","packageName":"clap","reasoning":"Confirm the clap crate."}'

# large queries from a file instead of shell-quoted JSON
octocode jev --input query.json
```

Exact field names per tool come from `octocode scheme <tool>` — the examples
above elide required fields for brevity.

## Commands

### Tools — one command per tool

Each command takes one positional raw JSON query (or `--input <file>`), and
`--compact` for single-line JSON output (default is indented). The JSON
contract is identical to the MCP server tool of the same name.

| Command | What it does |
|---|---|
| `localSearch` | Text/regex search across local files. |
| `localFetch` | Read a local file: pagination, ranges, match filtering, minification. |
| `astSearch` | Structural search (ast-grep), declarations, syntax trees, import graph. |
| `astRewrite` | Structural find-and-replace; previews before writing. |
| `lspSearch` | Definitions, references, hover, call/type hierarchy, diagnostics. |
| `ghSearch` | GitHub repository and code search. |
| `ghGetFileContent` | Read a GitHub file without cloning. |
| `ghSearchHistory` | Search PRs, issues, and commits. |
| `ghGetHistoryItem` | Read one PR, issue, commit, or comparison. |
| `ghCloneRepo` | Clone into the local cache for offline analysis. |
| `artifactSearch` | Package lookup/discovery across 8 registries. |
| `jev` | Judgment engine: gate, compare, or audit candidates. |

### System

| Command | What it does |
|---|---|
| `scheme [tool]` | No name: compact catalog of every tool with availability. With a name: the complete contract. `--view query` for the self-contained query schema; `--select FIELD=VALUE` to keep one union branch. |
| `config` | Show config file paths and set key names — values are never printed. `--check <key>` tests one key; `--json`. |
| `auth` | Auth status (default). `auth login` (device flow; `--refresh`, `--force`, `--hostname`), `auth logout`. |
| `install` | Install the MCP server into an IDE. `claude` aliases `claude-desktop`; `vscode` aliases `vscode-cline`. |
| `skill <args…>` | Pass-through to the `octocode skill` Node CLI. |
| `help` | Print help. |

Hidden maintenance commands (not part of the agent surface, still available):
`cache <status|clear>` and `lsp-server <list|install|uninstall|clean|status|which>`.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Empty result / no matches |
| `2` | Invalid input (also clap argument errors) |
| `3` | Not found |
| `4` | Auth required |
| `5` | Execution error |
| `6` | Partial result — the response carries a re-runnable `next.*` continuation |
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
| **Query interface** | Raw JSON per tool (`octocode <tool> '<json>'`), schemas via `scheme` | Delegates to the native binary |
| **Output** | Structured JSON (indented; `--compact` for one line) | Same — the Node CLI is a launcher |
| **Environments without Node** | ✓ Standalone | ✗ Node required |
| **Interactive UI** | Plain text | Menus, spinners, colored headers |
| **Auth flow** | Native GitHub device flow with keychain storage | Native interactive OAuth with keychain |

**Use `octocode-native`** for shell scripts, CI pipelines, environments without
Node, and fast config/auth checks.

**Use the Node CLI** for interactive IDE install and skill materialization —
every other command delegates to this binary.

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
- **No Node fallback for research or auth.** The binary terminates with an error rather than shelling out to Node. The `skill` command delegates to the Node CLI by design.
- **Strict clippy.** `unwrap_used = deny`, `dbg_macro = deny`.
- **clap v4 derive.** All argument parsing uses `#[derive(Parser)]` / `#[derive(Args)]` — no builder API.
- **Parse-time validation.** All enum-like string args use `value_parser` so bad values exit 2 with choices printed — never a runtime panic.
