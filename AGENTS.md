# AGENTS.md — Octocode Monorepo

Internal guide. When present, prefer each package's `ARCHITECTURE.md` / `AGENTS.md` / `docs/` over anything here.

---

## Dogfood — always, no exceptions

**This repo ships the tools. Use them on themselves.**

When working in this repo, reach for the local CLI, MCP, or a skill **first** — never raw shell/grep/find when Octocode can do it better.

```bash
OCTO='node packages/octocode/out/octocode.js'
$OCTO scheme                                    # live catalog
$OCTO scheme <name> --compact                   # schema before calling
```

| Need | Use |
|---|---|
| Search code / files / symbols / LSP | Local CLI (`$OCTO <toolName> '<json>'`) **or** Octocode MCP |
| GitHub code, PRs, history | same tools — `ghSearch`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem`, `ghCloneRepo` |
| Package discovery | `artifactSearch` |
| Research / trace / change impact | `octocode-research` skill |
| Architecture decisions | `octocode-architect` skill |
| Benchmarks / keep-discard | `octocode-eval-benchmark` skill |
| Evidence-driven reasoning crossroads | `octocode-jev-reasoning-loop` skill |
| Offload bulk to local Ollama | `octocode-subagent` skill |
| After any package change | rebuild → test via real CLI/MCP/skill path — not just compile |

**Skills are first-class** — wired to the same tools; your default entry point for research, architecture, and eval flows.

**Dogfood Jev where it changes the next action.** Follow [`skills/octocode-jev-reasoning-loop/SKILL.md`](skills/octocode-jev-reasoning-loop/SKILL.md): use the pure `jev` tool with one `{context, question}` per query for bounded relevance, conditions, claim support or alternatives. Context is an unread read-tool request `{tool, query}` or observed `{value}`. Results contain one answer and compact coverage metadata, not retrieved bodies. Batch up to five independent queries, repeating context explicitly. Partial pages cannot establish global absence. Candidate/question counts and upcoming assertions alone do not trigger calls. Use exact lookups/tests directly, reuse current evidence, and do not automatically chain scout → conditions → reasoning.

### Reflect after every tool/skill use

Note friction, gaps, or wrong defaults and log them (comment/issue) instead of silently working around — if dogfooding hurts, fix it. Raw findings → [`.octocode/GOTCHAS.md`](.octocode/GOTCHAS.md); durable Jev practice → [`.octocode/JEV.md`](.octocode/JEV.md).

---

## Architecture and data flow

```
 INTERFACES      octocode-mcp  ·  octocode (CLI)  ·  octocode-vscode  ·  octocode-pi-extension
                       └────────────────────────────┴─── depend on ───┐
 BRAIN           @octocodeai/octocode-native       (Rust runtime + consolidated distribution)
                       ├── contracts ──▶  @octocodeai/octocode-core    (schemas / descriptions / types — sibling repo)
                       ├── primitives ─▶  crates/engine               (Rust: ripgrep, AST, LSP, minify, secrets)
                       └── config   ───▶  @octocodeai/config           (env + home — zero-dep, single source)
```

**Flow:** A tool call arrives at MCP or CLI → the native Rust runtime validates, executes, secures, and shapes it → the interface registers or renders the result. Interfaces contain no tool business logic and have no TypeScript execution fallback.

**Contracts:** Public schemas, descriptions, and instructions live in `@octocodeai/octocode-core` (sibling repo `../octocode-mcp-host/packages/octocode-core`). Import from `…/schema` for names/schemas/relations and `…/mcp` for `buildMcpInstructions` / `buildCliToolContext`. Never hand-write tool guidance in interface packages.

**Config:** Everything flows through `@octocodeai/config`. Never reimplement `getOctocodeHome`, `propagateOctocodeEnv`, or `.env` parsing. Skills use injected `octocode-config.mjs`; packages import from `@octocodeai/config`.

---

## Packages

10 workspace packages + 1 external core. Each has its own `ARCHITECTURE.md` — read it.

### Core stack

| Package | npm name | Role |
|---|---|---|
| [`octocode-config`](packages/octocode-config) | `@octocodeai/config` | Zero-dep env/config loader. Single source for home, env, protected keys. Used by everything. |
| [`octocode-native`](packages/octocode-native) | `@octocodeai/octocode-native` | **Brain and distribution owner.** Two Rust crates: runtime policy/CLI/N-API plus reusable engine primitives. Publishes runtime at `.`/`./runtime` and primitives at `./engine` through one six-platform family. |
| [`octocode-extension-rust`](packages/octocode-extension-rust) | `@octocodeai/octocode-extension-rust` | Rust primitives for the Pi extension: filesystem snapshots, mutations, durability, line diff. Separate from the research engine. |
| `@octocodeai/octocode-core` *(external)* | sibling repo | All public tool contracts, schemas, descriptions, examples. Source of truth for what tools exist and how they're described. |

### Interfaces

| Package | npm name | Role |
|---|---|---|
| [`octocode-mcp`](packages/octocode-mcp) | `octocode-mcp` | Thin MCP stdio server: lifecycle → security → tool registration → sanitized output. No logic. |
| [`octocode`](packages/octocode) | `octocode` | CLI: `scheme <name>`, `skill`, `config`, `auth`, `lsp-server`, `install`. Use `node packages/octocode/out/octocode.js` in-repo. |
| [`octocode-vscode`](packages/octocode-vscode) | `octocode-mcp-vscode` | VS Code extension: GitHub OAuth, MCP install into Cursor/Windsurf/etc., token sync. |
| [`octocode-pi-extension`](packages/octocode-pi-extension) | `@octocodeai/pi-extension` | Pi integration: native tools, bundled CLI/MCP wiring, Awareness assets, prompts, harness hooks. Contracts under `src/contracts/`. |

### Support / platform

| Package | npm name | Role |
|---|---|---|
| [`octocode-skill-installer`](packages/octocode-skill-installer) | `@octocodeai/octocode-skill-installer` | Shared durable skill materialization: platform paths, links/junctions, conflict policy. Bundled into callers. |
| [`octocode-awareness`](packages/octocode-awareness) | `@octocodeai/octocode-awareness` | Coordination runtime: plans, locks, messages, memory, reflection, verification, hooks. Pi-facing subset via `…/host`. |
| [`octocode-benchmark`](packages/octocode-benchmark) | `@octocodeai/octocode-benchmark` | Internal evals: head-to-head comparisons, VRPT scoring. |

**Cross-cutting rules:**
- Pi contracts → `packages/octocode-pi-extension/src/contracts`
- Awareness host API → build Awareness before rebuilding Pi after any `…/host` change
- Local core changes → build sibling, refresh `file:` resolution, rebuild consumers

---

## Tools

Full reference: [`docs/OCTOCODE_TOOLS.md`](docs/OCTOCODE_TOOLS.md) · live: `$OCTO scheme`

| Family | Tools | Role |
|---|---|---|
| GitHub | `ghSearch` · `ghGetFileContent` · `ghSearchHistory` · `ghGetHistoryItem` · `ghCloneRepo` | Code/repo/tree discovery, exact reads, history, clone |
| Package | `artifactSearch` | Lookup across 8 ecosystems + source repo |
| Local | `localSearch` · `localFetch` | Text/regex search · exact/minified file reads |
| AST | `astSearch` | Syntax trees, symbols, structural match, topology (deps/dependents/cycles/reachability/dead-code) |
| AST rewrite | `astRewrite` | Preview structural edits; apply requires snapshot + unchanged file hashes |
| LSP | `lspSearch` | Definitions, references, callers/callees, types, diagnostics |

**Evidence rules:**
- `astSearch` topology edges are **candidates** — confirm with `lspSearch` references/callers before any delete claim
- Pagination: never drop results silently; always provide a schema-valid executable `next.*` continuation or an explicit terminal-limit diagnostic

**Field gotchas:** `localSearch` takes `path` (absolute) + `searchText` — no `operation`, no `directory`, no `maxResults`, no `limit` (use `maxFiles`, `pageSize`, or `maxMatchesPerFile`). Check live schema first: `$OCTO scheme <name> --compact`.

---

## Build

```bash
yarn build                                          # all packages
yarn workspace <pkg-name> <script>                  # single package
yarn test · yarn lint · yarn typecheck · yarn verify
yarn build:native:all · yarn platforms:check
```

**End-to-end after engine/native/CLI changes:**

```bash
yarn workspace @octocodeai/octocode-native build:dev
yarn workspace octocode build:dev        # or: yarn workspace octocode-mcp build:dev
$OCTO config --json && $OCTO scheme
```

`build:dev` skips clean + lint and builds both addons in debug mode. Verify by exit code — don't inspect `target/debug/` paths. Coverage floors are per-package ratchets in `vitest.config.*` — never lower them, raise when coverage improves. Rust tests: `yarn workspace @octocodeai/octocode-native test:rust`.

---

## Dev setup / publish

```bash
yarn devScript && yarn install    # local dev: resolve internal packages from workspace
```

**Before publishing:**

```bash
node ./scripts/prepublish.mjs --fix   # strip local workspace: resolutions
yarn install && yarn prepublish       # lockfile + final guard + readme sync
```

`prepublish.mjs` can also `--dry-run` (preview) or run without flags (check only). Packages version independently. See [`scripts/README.md`](scripts/README.md).

---

## Bash gotchas

| Pattern | Problem | Fix |
|---|---|---|
| `npx <pkg>@latest` | Prompts — hangs | `npx -y <pkg>@latest` |
| `npx pkg --version` | Downloads whole pkg | `npm view <pkg>@latest version` |
| Mixing fast + slow in one batch | Hang kills all | Isolate slow/network calls |
| `timeout` on macOS | GNU only | `gtimeout` or `perl -e 'alarm N; exec @ARGV' -- cmd` |

---

## Docs

| Area | Link |
|---|---|
| MCP | [`docs/OCTOCODE_MCP.md`](docs/OCTOCODE_MCP.md) |
| Tools | [`docs/OCTOCODE_TOOLS.md`](docs/OCTOCODE_TOOLS.md) |
| Config | [`docs/CONFIGURATION.md`](docs/CONFIGURATION.md) · [`docs/ADDING_CONFIG.md`](docs/ADDING_CONFIG.md) (contributor guide) |
| Security | [`docs/SECURITY.md`](docs/SECURITY.md) |
| CLI | [`packages/octocode/docs/OCTOCODE_CLI.md`](packages/octocode/docs/OCTOCODE_CLI.md) |
| Engine / LSP | [`LSP_SERVER_LIFECYCLE.md`](packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md) · [`SUPPORTED_LANGUAGES_AND_FEATURES.md`](packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md) |
| Research | [`docs/OCTOCODE_RESEARCH_MANIFEST.md`](docs/OCTOCODE_RESEARCH_MANIFEST.md) · [`docs/ROUTING_EVIDENCE_POSITION_PAPER.md`](docs/ROUTING_EVIDENCE_POSITION_PAPER.md) |
| Jev | [`.octocode/JEV.md`](.octocode/JEV.md) (agent protocol + economics) · [`docs/OCTOCODE_JEV.md`](docs/OCTOCODE_JEV.md) (contract + workflows) · [`docs/JEV_BENCHMARK.md`](docs/JEV_BENCHMARK.md) |
| Benchmarks | [`BENCHMARK.md`](packages/octocode-benchmark/skills/octocode-benchmark/references/BENCHMARK.md) · [`SCORING.md`](packages/octocode-benchmark/skills/octocode-benchmark/references/SCORING.md) |
| Skills (repo) | [`skills/`](skills/) → linked into [`.agents/skills/`](.agents/skills/) |
