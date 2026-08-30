# AGENTS.md — Octocode Monorepo

Default agent guide for this repo. Internals: each package’s `ARCHITECTURE.md` when present.

## Dogfood

This monorepo is the platform. Use what we ship — do not reinvent with host defaults.

| Need | Use | Not |
|---|---|---|
| Local code search / structure / files / content / LSP | Octocode MCP **or** `node packages/octocode/out/octocode.js tools …` — all local tools below | bare `find` / `grep` / `rg` / `cat` / `ls` |
| GitHub code, repos, PRs/commits, clone | same — all GitHub tools below | ad-hoc `gh` / raw API (except when Octocode is unavailable) |
| npm lookup | `npmSearch` | ad-hoc registry curls |
| Unified research | raw `tools <name>` invocation (CLI or MCP) | hand-rolled multi-tool scripts |
| Research / review / change flows | `octocode-research` skill | inventing search loops |
| Measure “did this help?” / keep-discard | `octocode-eval-benchmark` skill | vibe acceptance / editing graders to pass |
| Offload low-risk bulk to local Ollama | `octocode-subagent` (`references/local-ollama.md`) | cloud-only token burn / inventing Ollama loops |
| After a package change | rebuild → real CLI / MCP / skill path | claim done from compile alone |

If dogfooding hurts, fix or record it — do not silently bypass.

Method: Plan → TDD → `yarn workspace <pkg> test` → `yarn lint` → verify. No backward compat by default — refactor freely; add shims only when asked.

Access: `packages/*/src/`, `tests/`, `docs/` ✅ · `*.json`, `*.config.*`, `Cargo.toml`, `scripts/` ⚠️ ask · `.env*`, `node_modules/`, `dist/`, `out/`, `target/` ❌

## Architecture

```
 INTERFACES   octocode-mcp (stdio MCP)   octocode (CLI)   octocode-vscode (VS Code)
                    └─────────────────────┴── depend on ──┐
 BRAIN         @octocodeai/octocode-tools-core  (execution + schemas/descriptions)
                    ├── prompt ────▶  @octocodeai/octocode-core  (external: shared system prompt + output types)
                    ├── native   ──▶  @octocodeai/octocode-engine  (Rust/napi: search, minify, LSP, secrets)
                    └── config   ──▶  @octocodeai/config           (env/config loader — zero-dep, single source)
```

Tool execution, schemas, and descriptions live in tools-core; the shared system prompt currently comes from octocode-core; native primitives live in engine. Interface packages only register, render, and configure. Never duplicate `getOctocodeHome` or `.env` parsing — use `@octocodeai/config`.

## Packages

All workspace packages (7). Prefer package `ARCHITECTURE.md` / `AGENTS.md` / `docs/` over guessing.

| Package | npm name | What it is | Dig deeper |
|---|---|---|---|
| [`packages/octocode-config`](packages/octocode-config) | `@octocodeai/config` | Zero-dep env + config loader — single source for `getOctocodeHome`, `parseEnv`, `loadOctocodeEnv`, `propagateOctocodeEnv`, `loadOctocoderc`, `PROTECTED_KEYS`. Used by every package (`workspace:*`) and injected into skill scripts as `octocode-config.mjs`. CLI: `npx @octocodeai/config [--keys\|--check KEY]`. | package `src/` |
| [`packages/octocode-tools-core`](packages/octocode-tools-core) | `@octocodeai/octocode-tools-core` | Brain. All tool runners, GitHub/Octokit client, security, providers, credentials, session, config. Registry: `src/tools/toolConfig.ts`. Delegates home/env to `@octocodeai/config`; native work to engine. | [ARCHITECTURE](packages/octocode-tools-core/ARCHITECTURE.md) |
| [`packages/octocode-engine`](packages/octocode-engine) | `@octocodeai/octocode-engine` | Only Rust package (napi-rs) + TS LSP/security wrappers. Minify, ripgrep, AST structural search, secret detection, LSP pool. | [ARCHITECTURE](packages/octocode-engine/ARCHITECTURE.md) · [LSP lifecycle](packages/octocode-engine/docs/LSP_SERVER_LIFECYCLE.md) |
| [`packages/octocode-mcp`](packages/octocode-mcp) | `octocode-mcp` | Thin MCP stdio server: lifecycle → security → tool registration → sanitized output. No business logic. | [ARCHITECTURE](packages/octocode-mcp/ARCHITECTURE.md) · [docs/OCTOCODE_MCP.md](docs/OCTOCODE_MCP.md) |
| [`packages/octocode`](packages/octocode) | `octocode` | CLI — same tool runners as MCP via raw `tools <name>`, plus install/auth/MCP-marketplace, `skill`, `context`, `lsp-server`. Prefer `node packages/octocode/out/octocode.js` in this monorepo. | [ARCHITECTURE](packages/octocode/ARCHITECTURE.md) · [CLI](packages/octocode/docs/OCTOCODE_CLI.md) |
| [`packages/octocode-vscode`](packages/octocode-vscode) | `octocode-mcp-vscode` | VS Code / multi-editor management extension: GitHub OAuth, MCP install into Cursor/Windsurf/etc., token sync. | package README |
| [`packages/octocode-benchmark`](packages/octocode-benchmark) | `@octocodeai/octocode-benchmark` | Internal benchmarks/evals — head-to-head tool comparisons (octocode vs gh / gh+rtk / ast-grep), VRPT scoring. | [BENCHMARK](packages/octocode-benchmark/skills/octocode-benchmark/references/BENCHMARK.md) |

External (not in this workspace): `@octocodeai/octocode-core` (sibling `octocode-mcp-host`) — current source for the shared system prompt and reusable output types only. Public tool schemas and descriptions are owned here under `packages/octocode-tools-core/src/toolContract/`. Never hand-write tool guidance in interface packages.

## Tools

Full field-level reference: [`docs/OCTOCODE_TOOLS.md`](docs/OCTOCODE_TOOLS.md). Live catalog: `$OCTO tools --json`; read `$OCTO tools <name> --scheme --json --compact` before calling a tool. Compact schemas include `relations` for conditional and mutually exclusive fields.

**Full discovery catalog (17)** — releases and Discussions are opt-in through `ENABLE_RELEASES` and `ENABLE_DISCUSSIONS`; MCP also gates cloning with `ENABLE_CLONE`:

| Family | Tools | Role |
|---|---|---|
| GitHub | `ghSearchCode` · `ghGetFileContent` · `ghViewRepoStructure` · `ghSearchRepos` · `ghSearchPullRequests` · `ghSearchIssues` · `ghSearchCommits` · `ghListReleases` · `ghSearchDiscussions` · `ghCloneRepo` | Remote code/path search, file read, tree, repo discovery, PR search, issue search, commit history/compare, releases (`ENABLE_RELEASES`), discussions (`ENABLE_DISCUSSIONS`), clone (`ENABLE_CLONE` on MCP) |
| Package | `npmSearch` | npm package lookup + source repo |
| Local | `localSearchCode` · `localViewStructure` · `localFindFiles` · `localGetFileContent` | Text (text/regex/AST), tree, find-by-meta, file read (`ENABLE_LOCAL=false` disables the family) |
| Graph | `localAnalyzeGraph` | Bounded file-graph operations: dependencies, dependents, shortest path, cycles/SCCs, reachability, and dead-code candidates |
| LSP | `lspGetSemantics` | definition, references, callers/callees, symbols, types, diagnostics, … |

Evidence: search and `localAnalyzeGraph` import edges are **candidates**, not symbol proof — use graph operations for repository file topology, then confirm identity/usage with `lspGetSemantics` (`references`/`callers`) before a delete claim. Do not treat search relevance ranking as proof.

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
yarn workspace octocode build:dev            # also: yarn workspace octocode-mcp build:dev
OCTO='node packages/octocode/out/octocode.js'
$OCTO --help
$OCTO context --compact
$OCTO tools --json
$OCTO tools localSearchCode lspGetSemantics --scheme
```

Prefer `node packages/octocode/out/octocode.js` over global `octocode` / npx when validating monorepo changes. After engine or tools-core edits: rebuild the package, then `yarn workspace octocode build:dev`. `build:dev` skips clean + lint; engine uses debug (not `--release`).

## Dev setup and publish guard

See [`scripts/README.md`](scripts/README.md) for the script catalog. Use the root scripts as the source of truth; do not re-add package-local version sync helpers.

### `yarn devScript` — local dev: resolve internal packages from workspace

```bash
yarn devScript                         # adds workspace:* to root resolutions
yarn install                           # apply resolutions
```

Adds the internal packages and octocode-engine platform packages to the `resolutions` field in the root `package.json` so Yarn uses the local build for every consumer, including transitive ones:

| Package | Role |
|---|---|
| `@octocodeai/octocode-tools-core` | Brain / all tool runners |
| `@octocodeai/config` | Zero-dep env + config loader |
| `@octocodeai/octocode-core` | External shared system prompt + output types (sibling repo) |
| `@octocodeai/octocode-engine` | Rust/napi engine |
| `@octocodeai/octocode-engine-*` | Platform-native engine packages from `packages/octocode-engine/npm/*` |

Script: [`scripts/dev-setup.mjs`](scripts/dev-setup.mjs). Idempotent — safe to re-run.

### `scripts/prepublish.mjs` — publish prep: remove resolutions + align versions

Runs automatically as part of `yarn prepublish`, followed by the shared final guard at `packages/octocode/scripts/check-no-workspace-protocol.mjs` and `readme:sync`. Also callable directly:

```bash
node ./scripts/prepublish.mjs             # check only — exit 1 if issues found
node ./scripts/prepublish.mjs --fix       # remove workspace:* resolutions + align package/dependency versions
node ./scripts/prepublish.mjs --dry-run   # preview fixes without writing
```

Three checks:

1. **Resolutions** — root `package.json` must not have `workspace:*` for managed packages. Yarn rewrites these during publish and can produce wrong pinned versions in tarballs.
2. **Publish package versions** — publishable Octocode package versions and engine platform package versions must match the root `package.json` version.
3. **Dependency alignment** — every workspace package that pins a managed package (non-`workspace:*`) must match the package's current `version` in the repo (written as `^<version>` or exact). Packages not in this workspace (e.g. `@octocodeai/octocode-core`) are skipped.

**Typical flow before publishing any package:**

```bash
node ./scripts/prepublish.mjs --fix   # remove dev resolutions + align versions
yarn install                          # update lockfile
yarn prepublish                       # runs prepublish + shared final guard + readme sync
```

## Docs and references

| Area | Links |
|---|---|
| Global | [`docs/OCTOCODE_MCP.md`](docs/OCTOCODE_MCP.md) · [`docs/CONFIGURATION.md`](docs/CONFIGURATION.md) · [`docs/OCTOCODE_TOOLS.md`](docs/OCTOCODE_TOOLS.md) · [`docs/SECURITY.md`](docs/SECURITY.md) · [`docs/OCTOCODE_RESEARCH_MANIFEST.md`](docs/OCTOCODE_RESEARCH_MANIFEST.md) · [`docs/ROUTING_EVIDENCE_POSITION_PAPER.md`](docs/ROUTING_EVIDENCE_POSITION_PAPER.md) |
| CLI | [`OCTOCODE_CLI.md`](packages/octocode/docs/OCTOCODE_CLI.md) |
| Engine | [`LSP_SERVER_LIFECYCLE.md`](packages/octocode-engine/docs/LSP_SERVER_LIFECYCLE.md) · [`SUPPORTED_LANGUAGES_AND_FEATURES.md`](packages/octocode-engine/docs/SUPPORTED_LANGUAGES_AND_FEATURES.md) |
| Benchmarks | [`BENCHMARK.md`](packages/octocode-benchmark/skills/octocode-benchmark/references/BENCHMARK.md) · [`README.md`](packages/octocode-benchmark/README.md) · [`SCORING.md`](packages/octocode-benchmark/skills/octocode-benchmark/references/SCORING.md) |
| Context | [`docs/context/`](docs/context/) — [SEARCH_GUIDE](docs/context/SEARCH_GUIDE.md) · [LSP_GUIDE](docs/context/LSP_GUIDE.md) · [AGENT_RESEARCH_WORKFLOWS](docs/context/AGENT_RESEARCH_WORKFLOWS.md) · [RUST_BEST_PRACTICES](docs/context/RUST_BEST_PRACTICES.md) |
| Skills (repo) | [`skills/`](skills/) — linked into [`.agents/skills/`](.agents/skills/); all `octocode-*` skills dogfood from here |

## Config / env — single source

All env/config loading flows through `@octocodeai/config`. Never reimplement:

- `getOctocodeHome(env?)` — `OCTOCODE_HOME` → platform default
- `propagateOctocodeEnv({ cwd, trusted, env })` — global + project `.env` → `process.env`
- `parseEnv(text)` · `loadOctocoderc(home?)` · `PROTECTED_KEYS`

Skills: `./octocode-config.mjs` (injected at build). Packages: `import { … } from '@octocodeai/config'`.
