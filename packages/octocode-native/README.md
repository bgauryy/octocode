# octocode-native

> **Package name:** `@octocodeai/octocode-native`  
> **Directory:** `packages/octocode-native/`

Consolidated distribution for the Octocode native CLI and the `NativeRuntime`
Node addon. The CLI runs without Node; Node consumers load the runtime from `.`
or `./runtime`.

```sh
$ octocode --version
octocode 20.0.0

$ octocode schema
{"kind":"octocode.toolCatalog","toolCount":16,"tools":[…]}      # availability + compact fields per tool

$ octocode schema localSearch
{"name":"localSearch","shortDescription":"…","querySchema":{…},"run":"…"}  # complete tool contract

$ octocode localSearch '{"queries":[{"matchString":"ToolRuntime","path":"src/","resultView":"matchOnly","reasoning":"Locate the runtime entry."}]}'
{"results":[{"data":{"searchEngine":"rg","files":[…]}}]}
```

Every tool is a first-class command under its canonical name — the same name
and the same JSON query contract as the MCP server. There are no per-tool flag
wrappers and no aliases. `octocode schema` (tool contracts, descriptions,
examples, agent instructions) is served by the `octocode` npm launcher, which
joins the core-owned presentation with this binary's hidden machine `catalog`
(availability, field lists, enforcement fingerprint); the binary embeds no
presentation of its own.

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

On Linux x64 the shim selects `linux-x64-musl` when `/usr/bin/ldd` names musl,
or when Node's `process.report` has no glibc runtime version and `ldd --version`
names musl. musl on ARM64 is unsupported.

### From source (requires Rust)

```sh
# dev build
yarn workspace @octocodeai/octocode-native build:dev
# or
cargo build --manifest-path packages/octocode-native/Cargo.toml -p octocode-cli --bins

# release build (LTO + strip)
cargo build --manifest-path packages/octocode-native/Cargo.toml -p octocode-cli --bins --release
```

`scripts/build-native.cjs` builds the CLI + runtime addon in one Cargo
invocation, then stages them atomically.

Binary locations:
```sh
packages/octocode-native/target/debug/octocode
packages/octocode-native/target/release/octocode
```

## Architecture

```text
crates/cli ──────────┐
                    ├──▶ crates/runtime ───▶ crates/github
crates/runtime-napi ┘          │
       │                       └───────────▶ crates/engine
       └─ NativeRuntime addon
```

The runtime library owns policy, credentials, contracts, cancellation, tool
orchestration, and response shaping. The CLI crate owns both executables; the
runtime N-API crate owns Node conversion and lifecycle. The GitHub crate owns
protocol and transport services with explicit inputs, without runtime configuration
or credential discovery. The engine retains reusable algorithms as a plain Rust
library. All five Rust crates are internal (`publish = false`); one npm
distribution ships three artifacts per platform.

### Language boundary

The default release has 11 first-class source-language families and exactly 28
extensions: JavaScript, TypeScript, Rust, Python, C, C++, Assembly, Java,
Scala, Go, and C#. Structural search/rewrite, signatures, graph facts, syntax
inspection, and LSP grammar adapters derive from one registry. CUDA native
parsing is an optional compile-time grammar (`tree-sitter-cuda`), excluded from
the default build because its parse tables cost ~6.8 MiB of binary size for a
niche language; it can be re-enabled via that feature. CUDA `.cu`/`.cuh` files
still route to `clangd` for LSP navigation regardless. Built-in semantic
server routes cover 11 families and 27 extensions: CUDA uses `clangd`, while
generic Assembly requires trusted custom LSP configuration. Text search,
ordinary reads, generic best-effort minification, artifact lookup, and trusted
custom LSP configuration remain language-agnostic. See
[`docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md`](docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md). The direct Rust dependency necessity and footprint receipt is in
[`docs/engine/DEPENDENCY_AUDIT.md`](docs/engine/DEPENDENCY_AUDIT.md).

### npm / platform distribution layout

```
packages/octocode-native/
├─ crates/runtime/               ← pure ToolRuntime library
├─ crates/cli/                   ← CLI and regex worker binaries
├─ crates/runtime-napi/          ← runtime Node adapter
├─ crates/github/                ← GitHub protocol services
├─ crates/engine/                ← reusable primitives (Rust library)
├─ js/                           ← runtime loader
├─ bin/                          ← platform-selecting CLI launchers
├─ npm/                          ← six packages, each with three artifacts
│   ├─ darwin-arm64/
│   ├─ darwin-x64/
│   ├─ linux-arm64-gnu/
│   ├─ linux-x64-gnu/
│   ├─ linux-x64-musl/
│   └─ win32-x64-msvc/
└─ scripts/
    ├─ build-native.cjs          ← hosts build → atomic staging
    └─ check-platform-binaries.cjs
```

Build a single platform and copy binaries:
```sh
yarn workspace @octocodeai/octocode-native build:target darwin-arm64
# → builds in target/platforms/darwin-arm64/
# → stages all three artifacts into npm/darwin-arm64/
```

Build all platforms concurrently (cross targets need cargo-zigbuild + zig and
cargo-xwin; `--jobs N` overrides the CPU/memory-derived concurrency):
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
octocode schema
octocode schema localFetch
octocode schema ghSearchHistory --view query --select operation=commit   # workflow + one union branch

# local file read (paginated; exit 6 + a re-runnable next.* continuation in the JSON)
octocode localFetch '{"queries":[{"path":"src/cli/mod.rs","ranges":["1-50"],"reasoning":"Read the dispatch entry."}]}'

# continue a paginated read: re-run results[].data.next.continue.query verbatim
octocode localFetch '{"queries":[{"path":"src/cli/mod.rs","unit":"lines","offset":50,"reasoning":"Continue the read."}]}'

# lexical / regex search
octocode localSearch '{"queries":[{"matchString":"ToolRuntime","path":"src/","resultView":"matchOnly","reasoning":"Locate the runtime entry."}]}'

# structural AST match
octocode astSearch '{"queries":[{"operation":"match","path":"src/","pattern":"pub async fn $NAME","language":"rust","reasoning":"List async entry points."}]}'

# structural rewrite (preview first; apply requires snapshot + expectedHashes from the preview)
octocode astRewrite '{"queries":[{"path":"src/","language":"rust","ruleKind":"pattern","pattern":"dbg!($X)","rewrite":"$X","reasoning":"Strip debug macros."}]}'

# LSP — go to definition
octocode lspSearch '{"queries":[{"operation":"definition","path":"src/cli/mod.rs","symbolName":"dispatch","lineHint":244,"reasoning":"Jump to dispatch."}]}'

# read a remote GitHub file (no clone required)
octocode ghGetFileContent '{"queries":[{"owner":"cli","repo":"cli","path":"README.md","reasoning":"Read upstream docs."}]}'

# GitHub repository / code search
octocode ghSearchRepo '{"queries":[{"keywords":["ast-grep"],"reasoning":"Find pattern-matching repos."}]}'

# PR / issue / commit history
octocode ghSearchHistory '{"queries":[{"operation":"pullRequest","owner":"octocodeai","repo":"octocode","keywords":["fix"],"reasoning":"Find fix PRs."}]}'
octocode ghGetHistoryItem '{"queries":[{"operation":"pullRequest","owner":"octocodeai","repo":"octocode","number":42,"reasoning":"Read PR 42."}]}'

# package lookup
octocode artifactSearch '{"queries":[{"type":"crates","packageName":"clap","reasoning":"Confirm the clap crate."}]}'

# large queries from a file instead of shell-quoted JSON
octocode clasify --input query.json
```

Exact field names per tool come from `octocode schema <tool>` — the examples
above elide required fields for brevity.

## Commands

### Tools — one command per tool

Each command takes one JSON query (positional, `--input <file>`, or `--input -`
for stdin). On a terminal it prints the rendered text MCP clients read; on a
pipe, single-line JSON (`--json` forces JSON). The JSON contract is identical to
the MCP server tool of the same name.

| Command | What it does |
|---|---|
| `localSearch` | Text/regex search across local files. |
| `localFetch` | Read a local file: pagination, ranges, match filtering, minification. |
| `structureSearch` | Directory outlines and file discovery by name or metadata. |
| `astSearch` | Structural search (ast-grep), declarations, and syntax trees. |
| `astTopology` | Dependency graph analysis for paths, cycles, reachability, dead code, and drift. |
| `astRewrite` | Structural find-and-replace; previews before writing. |
| `lspSearch` | Definitions, references, hover, call/type hierarchy, diagnostics. |
| `ghSearchRepo` | GitHub repository search. |
| `ghSearchCode` | GitHub indexed code and path search. |
| `ghStructure` | GitHub repository tree browsing. |
| `ghGetFileContent` | Read a GitHub file without cloning. |
| `ghSearchHistory` | Search PRs, issues, and commits. |
| `ghGetHistoryItem` | Read one PR, issue, commit, or comparison. |
| `ghCloneRepo` | Clone into the local cache for offline analysis. |
| `artifactSearch` | Package lookup/discovery across 8 registries. |
| `clasify` | Apply Noul, Choice, or Score questions to one resource matrix or a batch of independent matrices. Requires a classification key (`OCTOCODE_CLASSIFICATION_API` or `classification.api` in `.octocoderc`); CLI calls without one report the missing key. |

### System

| Command | What it does |
|---|---|
| `schema [tool]` | Served by the npm launcher (see above); the binary prints where to run it. |
| `config` | Config paths, loaded key names, and warnings — values are never printed (`--json`). `set KEY VALUE` / `set KEY --stdin`, `unset KEY`, `check KEY`, `view`. |
| `auth` | `status` (`--json`), `login` (device flow; `--refresh`, `--force`, `--hostname`), `logout`. |
| `install` | Add the MCP server to an agent client: `--ide <id>` with an exact id from `--list`. |
| `skill <args…>` | Pass-through to the `octocode skill` Node CLI. |
| `help` | Print help. |

Hidden maintenance commands (not part of the agent surface, still available):
`cache <status|clear>` and `lsp-server <list|install|uninstall|clean|status|which>`.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Empty result / no matches |
| `2` | Invalid input, including any rejected batch row |
| `3` | Not found |
| `4` | Auth required |
| `5` | Execution error |
| `6` | Partial result: the response carries a re-runnable `next.*` continuation |
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
| **Query interface** | Raw JSON per tool (`octocode <tool> '<json>'`) | Same, plus `schema` views |
| **Output** | Text on a terminal, single-line JSON on a pipe | Same — the Node CLI is a launcher |
| **Environments without Node** | ✓ Standalone | ✗ Node required |
| **Interactive UI** | Plain text | Menus, spinners, colored headers |
| **Auth flow** | Native GitHub device flow with keychain storage | Native interactive OAuth with keychain |

**Use `octocode-native`** for shell scripts, CI pipelines, environments without
Node, and fast config/auth checks.

**Use the Node CLI** for interactive IDE install and skill materialization —
every other command delegates to this binary.

## Test

```sh
# all Rust libraries, CLI, and adapter tests
yarn workspace @octocodeai/octocode-native test:rust

# CLI integration tests only
cargo test --manifest-path packages/octocode-native/Cargo.toml -p octocode-cli --test integration cli::

# via yarn
yarn workspace @octocodeai/octocode-native test
```

## Key constraints

- **No NAPI in the CLI binary or runtime library.** Runtime N-API is isolated in `crates/runtime-napi`; the engine has no N-API.
- **No Node fallback for research or auth.** The binary terminates with an error rather than shelling out to Node. The `skill` command delegates to the Node CLI by design.
- **Strict clippy.** `unwrap_used = deny`, `dbg_macro = deny`.
- **clap v4 derive.** All argument parsing uses `#[derive(Parser)]` / `#[derive(Args)]` — no builder API.
- **Parse-time validation.** All enum-like string args use `value_parser` so bad values exit 2 with choices printed — never a runtime panic.
