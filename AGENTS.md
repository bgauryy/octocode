# AGENTS.md — Octocode Monorepo

Default agent guide for this repo. Internals: each package’s `ARCHITECTURE.md` when present.

## Dogfood

This monorepo is the platform. Use what we ship — do not reinvent with host defaults.

| Need | Use | Not |
|---|---|---|
| Local code search / structure / files / content / LSP | Octocode MCP **or** `node packages/octocode/out/octocode.js tools …` — all local tools below | bare `find` / `grep` / `rg` / `cat` / `ls` |
| GitHub code, repos, PRs/commits, clone | same — all GitHub tools below | ad-hoc `gh` / raw API (except when Octocode is unavailable) |
| npm lookup | `npmSearch` | ad-hoc registry curls |
| Unified research / OQL | CLI `search` (and `oqlSearch` when `ENABLE_OQL`) | hand-rolled multi-tool scripts |
| Research / review / change flows | `octocode-research` skill | inventing search loops |
| After a package change | rebuild → real CLI / MCP / skill path | claim done from compile alone |

If dogfooding hurts, fix or record it — do not silently bypass.

Method: Plan → TDD → `yarn workspace <pkg> test` → `yarn lint` → verify. No backward compat by default — refactor freely; add shims only when asked.

Access: `packages/*/src/`, `tests/`, `docs/` ✅ · `*.json`, `*.config.*`, `Cargo.toml`, `scripts/` ⚠️ ask · `.env*`, `node_modules/`, `dist/`, `out/`, `target/` ❌

## Architecture

```
 INTERFACES   octocode-mcp (stdio MCP)   octocode (CLI)   octocode-vscode (VS Code)
                    └─────────────────────┴── depend on ──┐
 BRAIN         @octocodeai/octocode-tools-core  (all tool execution logic)
                    ├── metadata ──▶  @octocodeai/octocode-core  (external: schemas, descriptions, system prompt)
                    ├── native   ──▶  @octocodeai/octocode-engine  (Rust/napi: search, minify, LSP, secrets)
                    └── config   ──▶  @octocodeai/config           (env/config loader — zero-dep, single source)
```

Logic lives in tools-core / octocode-core / engine. Interface packages only register, render, configure. Never duplicate `getOctocodeHome` or `.env` parsing — use `@octocodeai/config`.

## Packages

All workspace packages (7). Prefer package `ARCHITECTURE.md` / `AGENTS.md` / `docs/` over guessing.

| Package | npm name | What it is | Dig deeper |
|---|---|---|---|
| [`packages/octocode-config`](packages/octocode-config) | `@octocodeai/config` | Zero-dep env + config loader — single source for `getOctocodeHome`, `parseEnv`, `loadOctocodeEnv`, `propagateOctocodeEnv`, `loadOctocoderc`, `PROTECTED_KEYS`. Used by every package (`workspace:*`) and injected into skill scripts as `octocode-config.mjs`. CLI: `npx @octocodeai/config [--keys\|--check KEY]`. | package `src/` |
| [`packages/octocode-tools-core`](packages/octocode-tools-core) | `@octocodeai/octocode-tools-core` | Brain. All tool runners, GitHub/Octokit client, security, providers, credentials, session, config. Registry: `src/tools/toolConfig.ts`. Delegates home/env to `@octocodeai/config`; native work to engine. | [ARCHITECTURE](packages/octocode-tools-core/ARCHITECTURE.md) |
| [`packages/octocode-engine`](packages/octocode-engine) | `@octocodeai/octocode-engine` | Only Rust package (napi-rs) + TS LSP/security wrappers. Minify, ripgrep, AST structural search, secret detection, LSP pool. | [ARCHITECTURE](packages/octocode-engine/ARCHITECTURE.md) · [LSP lifecycle](packages/octocode-engine/docs/LSP_SERVER_LIFECYCLE.md) |
| [`packages/octocode-mcp`](packages/octocode-mcp) | `@octocodeai/mcp` | Thin MCP stdio server: lifecycle → security → tool registration → sanitized output. No business logic. | [ARCHITECTURE](packages/octocode-mcp/ARCHITECTURE.md) · [docs/OCTOCODE_MCP.md](docs/OCTOCODE_MCP.md) |
| [`packages/octocode`](packages/octocode) | `octocode` | CLI — same tool runners as MCP, plus install/auth/MCP-marketplace, `search` (OQL), `skill`, `context`, `lsp-server`. Prefer `node packages/octocode/out/octocode.js` in this monorepo. | [ARCHITECTURE](packages/octocode/ARCHITECTURE.md) · [CLI](packages/octocode/docs/OCTOCODE_CLI.md) · [OQL](packages/octocode/docs/OCTOCODE_QUERY_LANGUAGE.md) |
| [`packages/octocode-vscode`](packages/octocode-vscode) | `octocode-mcp-vscode` | VS Code / multi-editor management extension: GitHub OAuth, MCP install into Cursor/Windsurf/etc., token sync. | package README |
| [`packages/octocode-benchmark`](packages/octocode-benchmark) | `@octocodeai/octocode-benchmark` | Internal benchmarks/evals — flow benchmarks, AST grep comparisons, format support matrix. | [BENCHMARKS](packages/octocode-benchmark/docs/BENCHMARKS.md) |

External (not in this workspace): `@octocodeai/octocode-core` (sibling `octocode-mcp-host`) — single source for tool descriptions, schemas, and system prompt text. Never hand-write tool guidance in interface packages.

## Tools

Full field-level reference: [`docs/OCTOCODE_TOOLS.md`](docs/OCTOCODE_TOOLS.md). Live catalog: `$OCTO tools --json` (schemas: `$OCTO tools <name> --scheme`).

**Always-on (12)** — dogfood these via MCP or local CLI:

| Family | Tools | Role |
|---|---|---|
| GitHub | `ghSearchCode` · `ghGetFileContent` · `ghViewRepoStructure` · `ghSearchRepos` · `ghHistoryResearch` · `ghCloneRepo` | Remote code/path search, file read, tree, repo discovery, PR/commit history, clone (`ENABLE_CLONE` for clone) |
| Package | `npmSearch` | npm package lookup + source repo |
| Local | `localSearchCode` · `localViewStructure` · `localFindFiles` · `localGetFileContent` | Text (text/regex/AST), tree, find-by-meta, file read (`ENABLE_LOCAL=false` disables the family) |
| LSP | `lspGetSemantics` | definition, references, callers/callees, symbols, types, diagnostics, … |

**OQL / unified research**

- CLI: `$OCTO search` (read-only research lanes; see `$OCTO context --compact`)
- Tool: `oqlSearch` when `ENABLE_OQL` is on (targets: code, content, structure, files, semantics, repos, packages, PRs, commits, diff, research, graph) — details in [`OCTOCODE_QUERY_LANGUAGE.md`](packages/octocode/docs/OCTOCODE_QUERY_LANGUAGE.md)

Evidence: research analyze packets are **candidates** — upgrade with `target:graph` + `proof:"lsp"` before delete claims. Do not treat `sort:relevance` as proof.

## Build and local run

```bash
yarn build · yarn test · yarn lint · yarn typecheck · yarn verify
yarn workspace <pkg-name> <script>
yarn build:native:all · yarn platforms:check
yarn deps:dedupe · yarn deps:dedupe:fix
```

Coverage target 90% (Vitest + v8). Rust: `yarn workspace @octocodeai/octocode-engine test:rust`.

Local end-to-end (when changing engine, tools-core, or CLI):

```bash
yarn workspace @octocodeai/octocode-engine build:dev
yarn workspace @octocodeai/octocode-tools-core build
yarn workspace octocode build:dev            # also: yarn workspace @octocodeai/mcp build:dev
OCTO='node packages/octocode/out/octocode.js'
$OCTO --help
$OCTO context --compact
$OCTO tools --json
$OCTO tools localSearchCode lspGetSemantics --scheme
```

Prefer `node packages/octocode/out/octocode.js` over global `octocode` / npx when validating monorepo changes. After engine or tools-core edits: rebuild the package, then `yarn workspace octocode build:dev`. `build:dev` skips clean + lint; engine uses debug (not `--release`).

## Docs and references

| Area | Links |
|---|---|
| Global | [`docs/OCTOCODE_MCP.md`](docs/OCTOCODE_MCP.md) · [`docs/CONFIGURATION.md`](docs/CONFIGURATION.md) · [`docs/OCTOCODE_TOOLS.md`](docs/OCTOCODE_TOOLS.md) · [`docs/SECURITY.md`](docs/SECURITY.md) · [`docs/OCTOCODE_RESEARCH_MANIFEST.md`](docs/OCTOCODE_RESEARCH_MANIFEST.md) · [`docs/ROUTING_EVIDENCE_POSITION_PAPER.md`](docs/ROUTING_EVIDENCE_POSITION_PAPER.md) |
| CLI / OQL | [`OCTOCODE_CLI.md`](packages/octocode/docs/OCTOCODE_CLI.md) · [`OCTOCODE_QUERY_LANGUAGE.md`](packages/octocode/docs/OCTOCODE_QUERY_LANGUAGE.md) · [`OQL_LANGUAGE_REFERENCE.md`](packages/octocode/docs/OQL_LANGUAGE_REFERENCE.md) · [`OQL_RESULTS_AND_EVIDENCE.md`](packages/octocode/docs/OQL_RESULTS_AND_EVIDENCE.md) · [`OQL_INTERNALS.md`](packages/octocode/docs/OQL_INTERNALS.md) |
| Engine | [`LSP_SERVER_LIFECYCLE.md`](packages/octocode-engine/docs/LSP_SERVER_LIFECYCLE.md) |
| Benchmarks | [`BENCHMARKS.md`](packages/octocode-benchmark/docs/BENCHMARKS.md) |
| Context | [`docs/context/`](docs/context/) — [SEARCH_GUIDE](docs/context/SEARCH_GUIDE.md) · [OQL_RESEARCH_GRAPH_FLOW](docs/context/OQL_RESEARCH_GRAPH_FLOW.md) · [LSP_GUIDE](docs/context/LSP_GUIDE.md) · [AGENT_RESEARCH_WORKFLOWS](docs/context/AGENT_RESEARCH_WORKFLOWS.md) · [RUST_BEST_PRACTICES](docs/context/RUST_BEST_PRACTICES.md) |
| Skills (repo) | [`skills/`](skills/) — 1 skill, a folder with `SKILL.md`: research |

## Config / env — single source

All env/config loading flows through `@octocodeai/config`. Never reimplement:

- `getOctocodeHome(env?)` — `OCTOCODE_HOME` → platform default
- `propagateOctocodeEnv({ cwd, trusted, env })` — global + project `.env` → `process.env`
- `parseEnv(text)` · `loadOctocoderc(home?)` · `PROTECTED_KEYS`

Skills: `./octocode-config.mjs` (injected at build). Packages: `import { … } from '@octocodeai/config'`.
