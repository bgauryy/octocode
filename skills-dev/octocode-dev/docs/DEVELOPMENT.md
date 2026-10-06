# Developing Octocode

This page owns the monorepo map, the contract pipeline, build commands, development-only environment variables, and ownership rules. Repository-wide agent rules are in `<repo>/AGENTS.md`. The tool concept is in `<repo>/docs/OCTOCODE_PROTOCOL.md`.

## Runtime flow

```text
octocode CLI launcher ─┐
octocode-mcp (stdio) ──┼──▶ octocode-native (Rust ToolRuntime: validate → execute → secure → shape)
octocode-mcp-vscode ───┘          ├──▶ embedded contract from @octocodeai/config (generated from octocode-core)
                                  └──▶ configuration resolved in Rust, same rules as @octocodeai/config
```

`octocode-native` is the one owner of public tool validation, providers, security, batching, pagination, response shaping, and cancellation. Interfaces register, delegate, render, or offer interactive selection. There is no TypeScript tool fallback.

## Packages

Versions come from each `package.json`. Workspace packages version independently.

| Path | npm name | Version | Owns |
|---|---|---|---|
| sibling repo `octocode-mcp-host/packages/octocode-core` | `@octocodeai/octocode-core` | external | Every tool's Zod input schema, description, output schema, limits, and shared MCP/CLI instructions. Never executes tools. Nothing in this repo authors contract content. |
| `packages/octocode-config` | `@octocodeai/config` | 20.1.0 | The only tool-contract generator (`generate:tool-contract` writes `contract/` and `src/contracts/toolTypes.generated.ts`; re-exports core through `./schema` and `./mcp`). Also the configuration contract (`config-contract.json` → `<repo>/docs/generated/CONFIG_SETTINGS.md`), Octocode home, `.env` / `.octocoderc` loading, and protected keys. See [ADDING_CONFIG.md](ADDING_CONFIG.md). |
| `packages/octocode-native` | `@octocodeai/octocode-native` | 20.0.0 | Rust implementation of every public tool. One npm package plus six platform packages (`npm/darwin-arm64`, `darwin-x64`, `linux-arm64-gnu`, `linux-x64-gnu`, `linux-x64-musl`, `win32-x64-msvc`) with the native CLI binary and addons. Publishing: `<repo>/packages/octocode-native/docs/PUBLISHING.md`. |
| `packages/octocode-mcp` | `octocode-mcp` | 20.0.0 | Thin stdio MCP server: loads the addon, checks the core/native fingerprint, registers available tools (never CLI-only `ghCloneRepo` and `astRewrite`), forwards calls. See `<repo>/docs/OCTOCODE_MCP.md`. |
| `packages/octocode-mcp-cli` | `octocode-mcp-cli` (private) | 0.1.0 | Library that maps MCP tools and instructions to a Zod CLI spec, help text, generated TypeScript, and MCP tool definitions. Does not execute Octocode tools. |
| `packages/octocode` | `octocode` | 20.0.0 | Public Node launcher: delegates to the native binary; owns `octocode skill` and the install picker. See `<repo>/packages/octocode/docs/OCTOCODE_CLI.md`. |
| `packages/octocode-vscode` | `octocode-mcp-vscode` | 20.0.0 | VS Code extension: GitHub sign-in, token sync into MCP configs, MCP install across editors. Runs no research tools. |
| `packages/octocode-claude-plugin` | `@octocodeai/claude-plugin` | 0.1.0 | Claude Code manifest, local MCP launch config, public skills, GitHub CLI onboarding. The marketplace points to the npm package. See its `ARCHITECTURE.md`. |
| `packages/octocode-codex-plugin` | `@octocodeai/codex-plugin` | 0.1.0 | Codex plugin metadata, local MCP launch config, public skills, onboarding; reuses native auth and CLI skill staging. See its `ARCHITECTURE.md`. |
| `packages/octocode-skill-installer` | `@octocodeai/octocode-skill-installer` (private) | 0.1.0 | Library bundled into the CLI: canonical skill copies, per-platform links or copies, upgrades, conflict policy, atomic replacement. |
| `packages/octocode-benchmark` | `@octocodeai/octocode-benchmark` (private) | 20.0.0 | Unified agent benchmark: 30 pinned questions, Octocode MCP vs `rg` + `gh`, blind Opus judge. Start at `<repo>/packages/octocode-benchmark/compare/unified/README.md`. |
| `packages/octocode-agents-communication` | `@octocodeai/octocode-agents-communication` | 0.1.0 | Session identity, path leases, messages. |

Native Cargo crates under `crates/`:

| Crate dir | Cargo name | Owns |
|---|---|---|
| `runtime` | `octocode-native` | Tool catalog, validation, config resolution, GitHub auth, providers, security, response shaping, pagination, `clasify`; `build.rs` embeds `packages/octocode-config/contract/` |
| `engine` | `octocode-engine` | Primitives: ripgrep search, tree-sitter AST and rewrite, LSP client pool, minification, secret scanning, code graph |
| `github` | `octocode-github` | GitHub REST/GraphQL transport, bounded provider operations, credential traits |
| `cli` | `octocode-cli` | The native `octocode` binary: tool commands, `schema`, `config`, `auth`, `graph`, `skill`, `install` |
| `runtime-napi` | `octocode-runtime-napi` | N-API adapter the MCP server loads in-process |

Other folders:

- `skills/`: public Agent Skills installed by `octocode skill install`. `skills-beta/` holds tested unpublished skills; `skills-dev/` holds skills for work on this repo.
- `octocode-local-testing/` (not a package): `harness/` runs end-to-end suites against the built MCP server (`node octocode-local-testing/harness/run-all.mjs`); `validate/` holds reports; `repos/` holds pinned clones. See `<repo>/octocode-local-testing/README.md`. Benchmark results: `<repo>/docs/BENCHMARKS.md` (pending).

## Contract pipeline

1. **Author** in core: Zod schema, description, instructions, limits.
2. **Build core**, then run `yarn contracts:regen` at the repo root. It refreshes the `file:` copy of core (`yarn install`) and runs `@octocodeai/config generate:tool-contract`, the only generator. It needs `cargo install cargo-typify --version 0.8.0 --locked`.
3. **Output** lands in `packages/octocode-config/contract/` and `src/contracts/toolTypes.generated.ts`. It is committed and never hand-edited. `check:tool-contract` fails when it is stale.
4. **Rebuild native** (`yarn workspace @octocodeai/octocode-native build:dev`). `crates/runtime/build.rs` embeds `contract/` in place and fails on a fingerprint mismatch.

Until native is rebuilt, the MCP server refuses to start and CLI `schema` refuses to describe tools. `OCTOCODE_ALLOW_CONTRACT_DRIFT=1` downgrades that to a warning outside production. Release order: [RELEASE.md](RELEASE.md).

### Adding a tool field

1. Add it in core and regenerate as above.
2. Declare and implement it per the `field-effect-coverage.json` rule in [SKILL.md](../SKILL.md#hard-rules).
3. A public limit change trips the `public_response_and_tree_limits_are_pinned` test by design. Update it deliberately.
4. Check the result against [TOOL_QUALITY.md](TOOL_QUALITY.md), then update `<repo>/docs/OCTOCODE_TOOLS.md`.

A new configuration setting follows [ADDING_CONFIG.md](ADDING_CONFIG.md).

## Build, test, lint

Run from the repo root. Repo-wide tasks run through `node skills-dev/octocode-dev/scripts/dev.mjs <task>`; the task table is in [SKILL.md](../SKILL.md#task-runner).

- `yarn workspace @octocodeai/octocode-native build:all`: release native binaries for all platforms (clears old binaries first). Root `yarn build:native:all` runs it.
- `yarn contracts:regen`: regenerate the tool contract from core.

| Package | Test | Lint / format |
|---|---|---|
| native | `test` (scripts + vitest + `test:rust`), `test:rust` alone | `lint` (clippy `-D warnings`), `fmt:rust`, `fmt:rust:check`, `check:crate-boundaries`, `docs:claims` |
| config | `test`, `check:tool-contract`, `check:config-contract` | `lint` |
| mcp | `test`, `test:contracts` | `lint`, `format:check` |
| octocode, vscode, skill-installer, benchmark | `test` | `lint` |

## Development environment variables

These are not user settings and are not read from `.octocoderc`. User settings are in `<repo>/docs/CONFIGURATION.md`.

| Variable | Read by | Effect |
|---|---|---|
| `OCTOCODE_NATIVE_BIN` | `octocode` launcher | Absolute path to a native `octocode` binary used instead of the platform package |
| `OCTOCODE_NATIVE_BINDING` | `octocode-mcp` | Path to a candidate `.node` addon. Ignored when `NODE_ENV=production`; the bundled `dist` honors it only with `NODE_ENV` `development` or `test` |
| `OCTOCODE_ALLOW_CONTRACT_DRIFT` | `octocode-mcp`, CLI `schema` | `1` turns the fingerprint mismatch into a stderr warning; same `NODE_ENV` limits |
| `OCTOCODE_SKILL_DELEGATED` | native `octocode skill` | Internal recursion guard set when native delegates `skill` to the npm CLI; do not set it |
| `XDG_CONFIG_HOME` | native `octocode install` (Linux) | Locates IDE config directories (default `~/.config`); never moves the Octocode home |

## Ownership rules

- `crates/engine` exposes primitives, not tool policy.
- Skill filesystem behavior comes from `@octocodeai/octocode-skill-installer`.
- Each doc topic has one owner; see `<repo>/docs/README.md`. Per-package invariants are in each package `ARCHITECTURE.md`.
