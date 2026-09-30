# Developing Octocode

This page owns the monorepo map, the contract pipeline, build/test/lint commands, development-only environment variables, and ownership rules. Repository-wide agent rules (never commit, dogfood the tools) are in `<repo>/AGENTS.md`; the concept behind the tools is in `<repo>/docs/OCTOCODE_PROTOCOL.md`.

## Runtime flow

```text
octocode CLI launcher ─┐
octocode-mcp (stdio) ──┼──▶ octocode-native (Rust ToolRuntime: validate → execute → secure → shape)
octocode-mcp-vscode ───┘          ├──▶ embedded contract from @octocodeai/config (generated from octocode-core)
                                  └──▶ configuration resolved in Rust, same rules as @octocodeai/config
```

Public tool validation, providers, security, batching, pagination, response shaping and cancellation have one owner: `octocode-native`. Interfaces register, delegate or render; there is no TypeScript tool fallback.

## Packages

Versions are from each `package.json`; workspace packages version independently.

| Path | npm name | Version |
|---|---|---|
| (sibling repo `octocode-mcp-host/packages/octocode-core`) | `@octocodeai/octocode-core` | external |
| `packages/octocode-config` | `@octocodeai/config` | 20.1.0 |
| `packages/octocode-native` | `@octocodeai/octocode-native` | 20.0.0 |
| `packages/octocode-mcp` | `octocode-mcp` | 19.2.0 |
| `packages/octocode` | `octocode` | 19.2.0 |
| `packages/octocode-vscode` | `octocode-mcp-vscode` | 19.2.0 |
| `packages/octocode-skill-installer` | `@octocodeai/octocode-skill-installer` (private) | 0.1.0 |
| `packages/octocode-benchmark` | `@octocodeai/octocode-benchmark` (private) | 19.2.0 |
| `skills/octocode-agents-communication` | `@octocodeai/octocode-agents-communication` (private) | 0.1.0 |

**`@octocodeai/octocode-core` (external).** Authors every tool's Zod input schema, description, output schema, limits and the shared MCP/CLI instructions. It defines contracts and never executes tools. Nothing in this repo authors contract content.

**`@octocodeai/config`.** Two jobs. It is the only tool-contract generator: `generate:tool-contract` writes `contract/` (`tool-contract.json`, fixtures, provenance, `tool_types.rs`) and `src/contracts/toolTypes.generated.ts`, and re-exports core through `./schema` and `./mcp`. It also owns the configuration contract (`config-contract.json`, which generates `<repo>/docs/generated/CONFIG_SETTINGS.md`), Octocode home resolution, `.env` / `.octocoderc` loading and protected keys for Node consumers. See [ADDING_CONFIG.md](ADDING_CONFIG.md).

**`@octocodeai/octocode-native`.** The Rust implementation of every public tool, shipped as one npm package plus six platform packages (`npm/darwin-arm64`, `darwin-x64`, `linux-arm64-gnu`, `linux-x64-gnu`, `linux-x64-musl`, `win32-x64-msvc`), each carrying the native CLI binary and the addons. Cargo crates under `crates/`:

| Crate dir | Cargo name | Owns |
|---|---|---|
| `runtime` | `octocode-native` | Tool catalog, validation, config resolution, GitHub auth, providers, security, response shaping, pagination, `clasify`; `build.rs` embeds `packages/octocode-config/contract/` |
| `engine` | `octocode-engine` | Primitives: ripgrep search, tree-sitter AST and rewrite, LSP client pool, minification, secret scanning, code graph |
| `github` | `octocode-github` | GitHub REST/GraphQL transport, bounded provider operations, credential traits |
| `cli` | `octocode-cli` | The native `octocode` binary: tool commands, `scheme`, `config`, `auth`, `graph`, `skill`, `install` |
| `runtime-napi` | `octocode-runtime-napi` | N-API adapter the MCP server loads in-process |

The `./engine` subpath exposes primitives, never tool policy. Publishing is in `<repo>/packages/octocode-native/docs/PUBLISHING.md`.

**`octocode-mcp`.** Thin stdio MCP server. It loads the native addon, checks the core/native contract fingerprint, registers the available tools (never the CLI-only `ghCloneRepo` and `astRewrite`) and forwards calls. See `<repo>/docs/OCTOCODE_MCP.md`.

**`octocode`.** The public Node launcher. It delegates tool commands and management commands to the native binary, owns `octocode skill` (via the skill installer) and the interactive install picker. See the CLI guide (`<repo>/packages/octocode/docs/OCTOCODE_CLI.md`).

**`octocode-mcp-vscode`.** VS Code extension: GitHub sign-in, token sync into MCP configs, and MCP installation across supported editors. It runs no research tools.

**`@octocodeai/octocode-skill-installer`.** Private library bundled into the CLI: durable canonical skill copies, per-platform links or copies, upgrades, conflict policy and atomic replacement.

**`@octocodeai/octocode-benchmark`.** Private agent-vs-agent eval: a Claude agent with the Octocode MCP server against the same agent with `rg` and `gh`, graded by a blind judge. Start at its README (`<repo>/packages/octocode-benchmark/README.md`).

**`skills/`.** Public Agent Skills installed by `octocode skill install`; each folder owns its `SKILL.md`. `skills-beta/` holds tested but unpublished skills, `skills-dev/` skills for working on this repo. `skills/octocode-agents-communication` is also a workspace package (session identity, path leases, messages).

**`octocode-local-testing/`.** Not a package. `harness/` runs end-to-end suites against the built MCP server (`node octocode-local-testing/harness/run-all.mjs`); `validate/` holds validation reports and `repos/` the pinned clones (including the eval corpus). See its README (`<repo>/octocode-local-testing/README.md`); agent-vs-agent results are in `<repo>/docs/BENCHMARKS.md`.

## Contract pipeline

1. **Author** in core: Zod schema, description, instructions, limits.
2. **Build core**, then run `yarn contracts:regen` at the repo root. It refreshes the `file:` copy of core (`yarn install`) and runs `@octocodeai/config generate:tool-contract`, the only generator. It needs `cargo install cargo-typify --version 0.8.0 --locked`.
3. **Output** lands in `packages/octocode-config/contract/` and `src/contracts/toolTypes.generated.ts`. It is committed and never hand-edited; `check:tool-contract` fails when it is stale.
4. **Rebuild native** (`yarn workspace @octocodeai/octocode-native build:dev`). `crates/runtime/build.rs` embeds `contract/` in place and fails on a fingerprint mismatch.

Until native is rebuilt, the **MCP server refuses to start** and CLI `scheme` refuses to describe tools, because the core fingerprint differs from the native embed. `OCTOCODE_ALLOW_CONTRACT_DRIFT=1` downgrades that to a warning outside production; fix drift by regenerating, not by overriding.

Never hand-write a tool wire type (no TS interface, Zod copy, or serde query/result struct). Release gate: `yarn workspace @octocodeai/config check:core-contract-sync:published` (publish core first).

### Adding a tool field

1. Add it in core and regenerate as above.
2. Declare the new field or discriminator value in `packages/octocode-native/crates/runtime/src/contracts/field-effect-coverage.json` and implement it in the native tool.
3. Public limit changes also trip the `public_response_and_tree_limits_are_pinned` test by design; update it deliberately.
4. Check the result against the acceptance criteria in [TOOL_QUALITY.md](TOOL_QUALITY.md), then update `<repo>/docs/OCTOCODE_TOOLS.md`.

A new configuration setting (not a tool field) follows [ADDING_CONFIG.md](ADDING_CONFIG.md).

## Build, test, lint

Run from the repo root unless noted.

| Command | Does |
|---|---|
| `node skills-dev/octocode-dev/scripts/dev.mjs build:dev` | Fast parallel build of every workspace; native uses a debug build (`build:dev`) |
| `node skills-dev/octocode-dev/scripts/dev.mjs build` | Release build of every workspace |
| `yarn workspace @octocodeai/octocode-native build:all` | Release native binaries for all platforms (clears old binaries first); root `yarn build:native:all` runs it |
| `node skills-dev/octocode-dev/scripts/dev.mjs test` · `node skills-dev/octocode-dev/scripts/dev.mjs lint` · `node skills-dev/octocode-dev/scripts/dev.mjs typecheck` | Run each workspace's script in dependency order |
| `node skills-dev/octocode-dev/scripts/dev.mjs verify` | Dependency-declaration check plus each package's `verify` |
| `node skills-dev/octocode-dev/scripts/dev.mjs docs:verify` | Links, workflow references, tool catalog, config keys and publish contracts in docs |
| `yarn contracts:regen` | Regenerate the tool contract from core |

Per package:

| Package | Test | Lint / format |
|---|---|---|
| native | `test` (scripts + vitest + `test:rust`), `test:rust` alone | `lint:rust` (clippy `-D warnings`), `fmt:rust`, `fmt:rust:check`, `check:crate-boundaries`, `docs:claims` |
| config | `test`, `check:tool-contract`, `check:config-contract` | `lint` |
| mcp | `test`, `test:contracts` | `lint`, `format:check` |
| octocode, vscode, skill-installer, benchmark | `test` | `lint` |

Run one with `yarn workspace <npm name> <script>`. After any package change, rebuild and exercise the real CLI or MCP path, not just the compile: `node packages/octocode/out/octocode.js <tool> '<json>'`.

## Development environment variables

These are not user settings and are not read from `.octocoderc`. User settings are in `<repo>/docs/CONFIGURATION.md`.

| Variable | Read by | Effect |
|---|---|---|
| `OCTOCODE_NATIVE_BIN` | `octocode` launcher | Absolute path to a native `octocode` binary used instead of the platform package |
| `OCTOCODE_NATIVE_BINDING` | `octocode-mcp` | Path to a candidate `.node` addon. Ignored when `NODE_ENV=production`; the bundled `dist` honors it only with `NODE_ENV` `development` or `test` |
| `OCTOCODE_ALLOW_CONTRACT_DRIFT` | `octocode-mcp`, CLI `scheme` | `1` turns the fingerprint mismatch into a stderr warning; same `NODE_ENV` limits |
| `OCTOCODE_SKILL_DELEGATED` | native `octocode skill` | Internal recursion guard set when native delegates `skill` to the npm CLI; do not set it |
| `XDG_CONFIG_HOME` | native `octocode install` (Linux) | Locates IDE config directories (default `~/.config`); never moves the Octocode home |

## Ownership rules

- Public tool behavior lives only in `octocode-native` Rust.
- Interfaces may register, delegate, render, or offer interactive selection; they never implement tools or hand-write tool guidance.
- `crates/engine` and the `./engine` subpath expose primitives, not tool policy.
- Public contracts come from `@octocodeai/octocode-core`, through `@octocodeai/config`.
- Configuration comes from `@octocodeai/config` (Node) and its generated contract (Rust); never reimplement home resolution or `.env` parsing.
- Skill filesystem behavior comes from `@octocodeai/octocode-skill-installer`.
- Each doc topic has one owner; see the docs index (`<repo>/docs/README.md`).

## Related guides

- [ADDING_CONFIG.md](ADDING_CONFIG.md): adding settings, sections and credentials.
- [TOOL_QUALITY.md](TOOL_QUALITY.md): tool quality acceptance.
- [RELEASE.md](RELEASE.md): release checklist and gates.
- [scripts/README.md](../scripts/README.md): root automation scripts.
- Package `ARCHITECTURE.md` files: per-package ownership and invariants.
