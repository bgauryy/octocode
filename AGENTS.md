# AGENTS.md — Octocode Monorepo

Single source of AI-agent guidance. No per-package AGENTS.md. Per-package internals: each package's ARCHITECTURE.md.

## Architecture

```
 INTERFACES   octocode-mcp (stdio MCP)   octocode (CLI)   octocode-vscode (VS Code)
                    └─────────────────────┴── depend on ──┐
 BRAIN         @octocodeai/octocode-tools-core  (all tool execution logic)
                    ├── metadata ──▶  @octocodeai/octocode-core  (external: schemas, descriptions, system prompt)
                    ├── native   ──▶  @octocodeai/octocode-engine  (Rust/napi: search, minify, LSP, secrets)
                    └── config   ──▶  @octocodeai/config           (env/config loader — zero-dep, single source)
```

Rule: logic lives in octocode-tools-core / octocode-core / octocode-engine. Interface packages only register, render, configure.
`@octocodeai/config` is the single source for all env/config loading — never duplicate `getOctocodeHome` or `.env` parsing.

## Packages

packages/octocode-config (@octocodeai/config)
  Env + config loader — zero dependencies, cross-platform, published. Single source for getOctocodeHome,
  parseEnv, loadOctocodeEnv, propagateOctocodeEnv, loadOctocoderc, PROTECTED_KEYS.
  Used by: all packages (workspace:*) + skills (octocode-config.mjs injected at build — no npm needed).
  CLI: npx @octocodeai/config [--keys|--check KEY]

packages/octocode-tools-core (@octocodeai/octocode-tools-core)
  Brain. All tool runners, GitHub/Octokit client, security, providers, credentials, session, config.
  Delegates getDefaultOctocodeHome → @octocodeai/config (single source, no duplicate logic).

packages/octocode-engine (@octocodeai/octocode-engine)
  Native primitives (only Rust — napi-rs). Minify, AST search, ripgrep, secret detection, LSP pool.
  See packages/octocode-engine/ARCHITECTURE.md.

packages/octocode-mcp (octocode-mcp)
  MCP server (stdio). Thin: lifecycle → security → tool registration → sanitized output.
  See packages/octocode-mcp/ARCHITECTURE.md.

packages/octocode (octocode)
  CLI. Runs every tool from the terminal + manages install/auth/MCP-marketplace.

packages/octocode-vscode (octocode-mcp-vscode)
  VS Code extension. GitHub OAuth, multi-editor MCP install, token sync.

packages/octocode-pi-extension (@octocodeai/pi-extension)
  Pi harness. Bundles MCP server, injects Octocode context into the pi system prompt.
  Build: injects octocode-config.mjs into every skill scripts/ dir (standalone, no npm needed).

packages/octocode-benchmark (@octocodeai/octocode-benchmark)
  Benchmarks/evals. Flow benchmarks, AST grep comparisons, format support matrix.

@octocodeai/octocode-core is an external dep (sibling repo octocode-mcp-host) — single source of all
tool descriptions, schemas, and system prompt text. Never hand-write tool guidance in interface packages.

## Tools (13)

GitHub: `ghSearchCode` · `ghGetFileContent` · `ghViewRepoStructure` · `ghSearchRepos` · `ghHistoryResearch` · `ghCloneRepo` (needs ENABLE_CLONE)
Package: `npmSearch`
Local (ENABLE_LOCAL=false disables): `localSearchCode` · `localViewStructure` · `localFindFiles` · `localGetFileContent` · `localBinaryInspect`
LSP: `lspGetSemantics`

## Build, test, lint

`yarn build` · `yarn test` · `yarn lint` · `yarn typecheck` · `yarn verify`
Per-package: `yarn workspace <pkg-name> <script>`
Native Rust: `yarn build:native:all` · `yarn platforms:check`
Workspace deps: `yarn local:check` · `yarn local:fix`
Publish: `yarn sync:version:publish`

Coverage target 90% (Vitest + v8). Rust: `yarn workspace @octocodeai/octocode-engine test:rust`.

## Local dev — workspace deps

Internal deps are version pins resolving to npm. To consume local source:

```bash
yarn local:fix                                               # switch internal deps → workspace:*
yarn workspace @octocodeai/mcp build:dev                     # builds tools-core then MCP server
yarn workspace octocode build:dev                            # builds tools-core then CLI
yarn workspace @octocodeai/octocode-engine build:dev         # Rust debug build + TS wrappers
node packages/octocode/out/octocode.js --help                # drive end-to-end
yarn sync:version:publish && yarn local:check                # before publish: restore pins
```

build:dev skips clean + lint; engine uses debug mode (not --release). MCP and CLI build:dev rebuild octocode-tools-core first automatically.

## Working in this repo

Methodology: Plan → TDD → `yarn workspace <pkg> test` → `yarn lint` → verify. Use octocode-local MCP tools for research.

No backward compat by default — refactor freely, delete dead paths, add shims only when asked.

Local skills: .agents/skills/ — octocode (architecture) · rust-package-node (napi-rs).
Use octocode-awareness (.agents/skills/octocode-awareness) for cross-run memory.

Access: packages/*/src/, tests/, docs/ ✅ · *.json, *.config.*, Cargo.toml, scripts/ ⚠️ ask · .env*, node_modules/, dist/, out/, target/ ❌

## Docs (docs/)

docs/OCTOCODE_MCP.md · docs/CONFIGURATION.md · docs/AUTHENTICATION.md — MCP overview, env vars, GitHub token/OAuth
docs/OCTOCODE_TOOLS.md — all 13 tools, behavior, params, clone workflow
docs/OCTOCODE_CLI.md · docs/OCTOCODE_QUERY_LANGUAGE.md — CLI commands/flags, OQL syntax
docs/SECURITY.md · docs/LSP_SERVER_LIFECYCLE.md — secret redaction, path validation, LSP lifecycle, 151-ext format matrix
release/RELEASE_GUIDE.md — versioning, publish checklist
docs/context/ — search guide, OQL graph flow, LSP guide, agent research workflows

## Config / Env — single source rule

All env and config loading flows through `@octocodeai/config`. Never reimplement:
- `getOctocodeHome(env?)` — cross-platform home dir (OCTOCODE_HOME → platform default)
- `propagateOctocodeEnv({ cwd, trusted, env })` — load global + project .env into process.env
- `parseEnv(text)` — strict dotenv parser
- `loadOctocoderc(home?)` — load .octocoderc (JSON with comments)
- `PROTECTED_KEYS` — keys .env must never override

Skills: import via `./octocode-config.mjs` (injected at build, no npm). Packages: `import { … } from '@octocodeai/config'`.
