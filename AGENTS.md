# AGENTS.md — Octocode Monorepo

Internal guide. When present, prefer each package's `ARCHITECTURE.md` / `AGENTS.md` / `docs/` over anything here.

---

## Git — hard rules

- **NEVER `git commit`.** Leave changes in the working tree; the human (or the checkpoint bot) owns commits. A background process runs `git add -A` continuously, so any `git commit` sweeps unrelated in-flight work into your commit under the wrong message. Never run it.
- **NEVER `git stash`.** It silently hides other sessions' uncommitted work and races the checkpoint bot. Use targeted reads/edits instead; to compare against a baseline, read from `git show <rev>:<path>` — do not stash.

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
| Evidence-driven semantic crossroads | `octocode-clasify` skill |
| Offload bulk to local Ollama | `octocode-subagent` skill |
| After any package change | rebuild → test via real CLI/MCP/skill path — not just compile |

**Skills are first-class** — wired to the same tools; your default entry point for research, architecture, and eval flows.

**Dogfood `clasify` where it changes the next action.** Follow [`skills/octocode-clasify/SKILL.md`](skills/octocode-clasify/SKILL.md): prefer `resources[] × questions[]` for a shared question set so each resource is captured once and every row carries both IDs. Use `queries[]` only for independent matrices whose cross-product would be wrong. Context is an unread read-tool request `{tool,query}` or observed `{value}`. Keep matrices within 25 cells. The runtime preserves ordered same-resource pages without a hidden reducer; follow `next.clasify` and retain partial/error pages. Results contain typed Noul, Choice, or Score answers and compact coverage metadata, not retrieved bodies. Partial coverage cannot establish global absence. Candidate/question counts and upcoming assertions alone do not trigger calls. Use exact lookups/tests directly, reuse current evidence, and verify semantic conclusions against source evidence.

### Reflect after every tool/skill use

Note friction, gaps, or wrong defaults and log them (comment/issue) instead of silently working around — if dogfooding hurts, fix it. Raw findings → [`.octocode/GOTCHAS.md`](.octocode/GOTCHAS.md); current semantic-assessment practice → [`docs/OCTOCODE_CLASIFY.md`](docs/OCTOCODE_CLASIFY.md); frozen Jev history → [`.octocode/JEV.md`](.octocode/JEV.md).

---

## Architecture and data flow

```
 AUTHORING       @octocodeai/octocode-core   (sibling repo — Zod schemas, descriptions, instructions)
                       │  yarn contracts:regen   (the ONLY generation step)
                       ▼
 CONTRACT HUB    @octocodeai/config          ./schema ./mcp (re-export core) + GENERATED:
                       │                       contract/  tool-contract.json · contract-fixtures.json · provenance.json
                       │                                  tool-types.schema.json · tool_types.rs (Rust types)
                       │                       src/contracts/toolTypes.generated.ts (TS types)
                       ├──────────────▶ INTERFACES  octocode-mcp · octocode (CLI) · octocode-vscode · octocode-pi-extension
                       │                             (import contracts + types from @octocodeai/config/schema|mcp)
                       └──────────────▶ BRAIN       @octocodeai/octocode-native — build.rs embeds contract/ in place
                                                     ├── crates/runtime  (validate · execute · secure · shape)
                                                     └── crates/engine   (ripgrep, AST, LSP, minify, secrets)
```

**Flow:** A tool call arrives at MCP or CLI → the native Rust runtime validates, executes, secures, and shapes it → the interface registers or renders the result. Interfaces contain no tool business logic and have no TypeScript execution fallback.

### Contracts and types — ONE place, ONE pipeline

| Step | Where | Rule |
|---|---|---|
| 1. Author | `@octocodeai/octocode-core` (`../octocode-mcp-host/packages/octocode-core`) | Every tool schema (Zod), description, instruction, and limit. Nothing else authors contract content. |
| 2. Generate | build core, then `yarn contracts:regen` (repo root) | Refreshes the `file:` core **copy** (`yarn install`), then runs `@octocodeai/config generate:tool-contract` — the **only** generator. Needs `cargo install cargo-typify --version 0.8.0 --locked`. |
| 3. Output | `packages/octocode-config/contract/` + `src/contracts/toolTypes.generated.ts` | Committed, never hand-edited. `check:tool-contract` (in `build`/`lint`) fails when stale. |
| 4a. TS consumers | `@octocodeai/config/schema` · `@octocodeai/config/mcp` | Zod schemas + generated types (`<Tool>Query`, `<Tool>Input`, `<Tool>Output`, `ToolQuery<N>`). Never import core directly. |
| 4b. Native | `crates/runtime/build.rs` | Embeds `contract/` **in place** (no copy); `contracts::tool_types` includes `contract/tool_types.rs`. Build fails on a fingerprint mismatch; cargo rebuilds when `contract/` changes. |

**Hard rules**
- Change a contract → edit core → `yarn contracts:regen` → rebuild. That is the whole change; there is no native script, copy, or pin to update. Rebuild native (`build:<platform>` **and** `build:addon`) right after every regen: until then the MCP server fails closed on the core/native fingerprint mismatch and new MCP sessions cannot start.
- **Never hand-write a tool wire type** — no TS interface/Zod copy in interfaces, no serde query/result struct in native. Native tools parse rows straight into the generated `<Tool>Query` and build continuations from it; they may add accessor `impl` blocks (e.g. `usize` getters) on generated types, nothing more.
- Generated-output payloads that core leaves open (`unknown[]`) stay open — tighten the Zod output schema in core, don't add a Rust/TS shape.
- The only native-side follow-up a contract change can force: a **new** field or discriminator value must be declared in `crates/runtime/src/contracts/field-effect-coverage.json` (and implemented). Public limit changes also trip `public_response_and_tree_limits_are_pinned` by design.
- Name a vocabulary shared across tools in core with `.meta({ title: "Name" })` — generated type names come only from core titles/ids or structure; an unnamed recursive schema fails generation.
- Drift is fail-closed: MCP refuses to start and CLI `scheme` refuses to describe tools when core's fingerprint ≠ the native embed (`OCTOCODE_ALLOW_CONTRACT_DRIFT=1` overrides outside production). Fix by regenerating, not overriding.
- Release gate: `yarn workspace @octocodeai/config check:core-contract-sync:published` — publish core first.
- Never hand-write tool guidance in interface packages.

**Config:** Everything flows through `@octocodeai/config`. Never reimplement `getOctocodeHome`, `propagateOctocodeEnv`, or `.env` parsing. Skills use injected `octocode-config.mjs`; packages import from `@octocodeai/config`.

---

## Packages

Workspace packages (`packages/*`) and one external core. Each package has its own `ARCHITECTURE.md` and `README.md`. Full overview: [`docs/PACKAGES.md`](docs/PACKAGES.md).

### Core stack

| Package | npm name | Role |
|---|---|---|
| [`octocode-config`](packages/octocode-config) | `@octocodeai/config` | **Content/context layer + the single contract generator.** Zero-dep env/config loader (`.` entry; single source for home, env, protected keys), the tool-contract hub (`./schema`, `./mcp` re-export core), and the generated `contract/` (embed JSON, fixtures, provenance, Rust `tool_types.rs`) + TS tool types. Every interface and native consume contracts from here. |
| [`octocode-native`](packages/octocode-native) | `@octocodeai/octocode-native` | **Brain and distribution owner.** Two Rust crates: runtime policy/CLI/N-API plus reusable engine primitives. Publishes runtime at `.`/`./runtime` and primitives at `./engine` through one six-platform family. |
| [`octocode-extension-rust`](packages/octocode-extension-rust) | `@octocodeai/octocode-extension-rust` | Rust primitives for the Pi extension: filesystem snapshots, mutations, durability, line diff. Separate from the research engine. |
| `@octocodeai/octocode-core` *(external)* | sibling repo | Authors all public tool contracts (Zod schemas, descriptions, examples). Source of truth; consumed only through `@octocodeai/config` after `yarn contracts:regen`. |

### Interfaces

| Package | npm name | Role |
|---|---|---|
| [`octocode-mcp`](packages/octocode-mcp) | `octocode-mcp` | Thin MCP stdio server: lifecycle → security → tool registration → sanitized output. No logic. |
| [`octocode`](packages/octocode) | `octocode` | CLI: one command per tool (`<toolName> '<json>'`) + `scheme`, `skill`, `config`, `login`/`logout`/`auth`, `install`. Use `node packages/octocode/out/octocode.js` in-repo. |
| [`octocode-vscode`](packages/octocode-vscode) | `octocode-mcp-vscode` | VS Code extension: GitHub OAuth, MCP install into Cursor/Windsurf/etc., token sync. |
| [`octocode-pi-extension`](packages/octocode-pi-extension) | `@octocodeai/pi-extension` | Pi integration: native tools, bundled CLI/MCP wiring, host state, prompts, harness hooks. Contracts under `src/contracts/`. |

### Support / platform

| Package | npm name | Role |
|---|---|---|
| [`octocode-skill-installer`](packages/octocode-skill-installer) | `@octocodeai/octocode-skill-installer` *(private)* | Shared durable skill materialization: platform paths, links/junctions, conflict policy. Bundled into callers. |
| [`octocode-agents-communication`](packages/octocode-agents-communication) | `@octocodeai/octocode-agents-communication` *(private)* | Session identity, advisory path leases, and direct messages. The skill folder ships the Rust CLI. Unpublished. |
| [`octocode-benchmark`](packages/octocode-benchmark) | `@octocodeai/octocode-benchmark` *(private)* | Internal evals: head-to-head comparisons, VRPT scoring. Ships the `octocode-benchmark` skill. |
| [`octocode-jev-lab`](packages/octocode-jev-lab) | `@octocodeai/jev-lab` *(private)* | Direct Jev/clasify provider probe for latency and multi-resource experiments, bypassing the runtime adapter. `yarn jev:probe --input <manifest>`. |

**Cross-cutting rules:**
- Pi contracts → `packages/octocode-pi-extension/src/contracts`
- Local core changes → build core → `yarn contracts:regen` (refreshes the `file:` copy + regenerates config `contract/`) → rebuild consumers

---

## Tools

13 catalog tools. Full reference: [`docs/OCTOCODE_TOOLS.md`](docs/OCTOCODE_TOOLS.md) · handoffs: [`docs/TOOL_DATA_CONTRACT.md`](docs/TOOL_DATA_CONTRACT.md) · live: `$OCTO scheme`. `ghCloneRepo` is CLI-only; MCP registers the rest when their availability gates pass.

| Family | Tools | Role |
|---|---|---|
| GitHub | `ghSearch` · `ghGetFileContent` · `ghSearchHistory` · `ghGetHistoryItem` · `ghCloneRepo` | Code/repo/tree discovery, exact reads, history, clone |
| Package | `artifactSearch` | Lookup across 8 ecosystems + source repo |
| Local | `localSearch` · `localFetch` | Text/regex search · exact/minified file reads |
| AST | `astSearch` | Files, syntax trees, symbols, structural match |
| AST topology *(beta)* | `astTopology` | deps/dependents/path/cycles/reachability/dead-code/drift — `OCTOCODE_BETA=1` |
| AST rewrite *(beta)* | `astRewrite` | Preview structural edits; apply requires snapshot + unchanged file hashes — `OCTOCODE_BETA=1` |
| LSP | `lspSearch` | Definitions, references, callers/callees, types, diagnostics |
| Semantic | `clasify` | Judge supplied context or screen unread resources — needs `OCTOCODE_CLASSIFICATION_API` |

**Evidence rules:**
- `astTopology` edges are **candidates** — confirm with `lspSearch` references/callers before any delete claim
- Pagination: never drop results silently; always provide a schema-valid executable `next.*` continuation or an explicit terminal-limit diagnostic

**Field gotchas:** `localSearch` takes `path` (absolute) + `searchText` — no `operation`, no `directory`, no `maxResults`, no `limit` or `maxFiles` (use `pageSize` or `maxMatchesPerFile`). Check live schema first: `$OCTO scheme <name> --compact`.

---

## Skills

Three skill trees. Entries in [`.agents/skills/`](.agents/skills/) (gitignored) must be **symlinks** to the canonical folder, never copies; edit the canonical source.

| Tree | What it is |
|---|---|
| [`skills/`](skills/) | **Public skills.** Published with the CLI and Pi bundle. `octocode skill install <name>` installs from here. |
| [`skills-beta/`](skills-beta/) | **Tested skills.** Not published and not bundled. |
| [`skills-dev/`](skills-dev/) | **Local development.** For working on this repository. Not published. |

### Public — [`skills/`](skills/)

| Skill | Use when |
|---|---|
| `octocode-research` | A code claim needs evidence: callers, imports, wiring, regressions, GitHub, change impact |
| `octocode-architect` | Architecture decision/refactor needs evidence on boundaries, contracts, flow, coupling, blast radius |
| `octocode-brainstorming` | Unresolved idea needs options, feasibility, scope exploration |
| `octocode-exploratory-thinking` | Exploratory or out-of-the-box thinking, or a named substance as a presence, before research or brainstorming |
| `octocode-rfc-generator` | Consequential architecture/migration/public-contract change needs a reviewed decision |
| `octocode-eval-benchmark` | Measuring whether a change helped: KPIs, baselines, keep/discard gates |
| `octocode-clasify` | Judge supplied context or screen unread resources before reading |
| `octocode-clean-agentic-code` | Behavior-preserving cleanup: dead exports, shims, duplicates, stale docs/tests, agent residue |
| `octocode-roast` | Blunt evidence-backed critique ranking smells and debt |
| `octocode-documentation` | Create/repair/review READMEs, API docs, guides, ADRs, runbooks |
| `octocode-prompt-optimizer` | Prompts, agent contracts, MCP instructions, tool/schema descriptions must change behavior |
| `octocode-skills` | Find, compare, review, create, install, sync, or tune skills |
| `octocode-subagent` | Independent lanes justify delegation: parallel workers, local Ollama, A2A |
| `octocode-scraping` | Fetch public URLs / crawl a site into a local corpus |
| `octocode-chrome-devtools` | Real browser needed: JS-rendered pages, DOM, HAR, console, auth sessions |

Package-owned skills, not in `skills/`: `octocode-benchmark` ([`packages/octocode-benchmark/skills/`](packages/octocode-benchmark/skills/)), `octocode-agents-communication` ([`packages/octocode-agents-communication/skills/`](packages/octocode-agents-communication/skills/)). Public folder contract: [`skills/README.md`](skills/README.md).

### Tested — [`skills-beta/`](skills-beta/) (not published)

No skill is in trial.

### Local development — [`skills-dev/`](skills-dev/) (not published; for working on this repository)

| Skill | Use when |
|---|---|
| [`octocode-dev`](skills-dev/octocode-dev/) | Auditing/hardening a tool end to end: core schema + descriptions ↔ Rust impl, data flow, MCP/CLI surfaces, docs. `scripts/tool-inventory.mjs`; reports under `.octocode/octocode-dev/` |
| [`rust-best-practices`](skills-dev/rust-best-practices/) | Rust choices are open: crates, error shape, module/workspace layout, cargo profiles, deps |
| [`ast-best-practices`](skills-dev/ast-best-practices/) | Rust code parsing with tree-sitter / oxc / ast-grep; engine AST map + known defects; `scripts/adversarial-fixtures.mjs` |
| [`lsp-best-practices`](skills-dev/lsp-best-practices/) | LSP client code (Rust/tokio) or trustworthy `lspSearch` answers; engine LSP map + known defects |

Update a skill's `references/octocode-known-defects.md` when you fix or find an engine defect in its area.

---

## Build

```bash
yarn build:dev                                      # FAST local full build (debug) — the default
yarn build                                          # all packages, RELEASE (slow — see below)
yarn workspace <pkg-name> <script>                  # single package
yarn test · yarn lint · yarn typecheck · yarn verify
yarn build:native:all · yarn platforms:check        # 6-platform cross-compile (publish only)
yarn docs:verify · yarn health:check · yarn deps:dedupe   # docs links, workspace health, dep dedupe
```

**Use `yarn build:dev` locally** (debug native + extension-rust + TS). `yarn build` compiles in
**release**, and `octocode-native` rebuilds its Rust dep graph **~3× per run** — the runtime crate
as a CLI bin (`--no-default-features`) **and** as a napi cdylib (`--features napi-addon`), plus the
engine addon (`--features portable-default,napi-addon`). Those are three distinct feature sets Cargo
can't share, so a cold release build is ~25 min (dominated by the engine crate at `codegen-units=1`).
Reserve `yarn build` for release/perf-representative artifacts; `build:dev` for everything else.
`[profile.dev]` emits line-tables-only debuginfo and `split-debuginfo="unpacked"` (skips macOS
`dsymutil`); deps carry none. For optimized-but-fast local artifacts use `--profile profiling`
(release opt, LTO off, debuginfo kept). Do **not** commit `.cargo/config.toml` lld/sccache blocks —
on Apple Silicon the default `ld` is already fast and sccache can't cache our cdylib/bin (they stay
opt-in; oxc/ruff/uv/biome commit no alternative linker either).

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

`prepublish.mjs` can also `--dry-run` (preview) or run without flags (check only). Packages version independently; core publishes **before** packages that embed its contracts. Full gated order: [`docs/RELEASE.md`](docs/RELEASE.md) · scripts: [`scripts/README.md`](scripts/README.md).

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

Index: [`docs/README.md`](docs/README.md). `out/docs` copies are build output — edit `docs/`.

| Area | Link |
|---|---|
| Packages | [`docs/PACKAGES.md`](docs/PACKAGES.md) |
| MCP | [`docs/OCTOCODE_MCP.md`](docs/OCTOCODE_MCP.md) |
| Tools | [`docs/OCTOCODE_TOOLS.md`](docs/OCTOCODE_TOOLS.md) · [`docs/TOOL_DATA_CONTRACT.md`](docs/TOOL_DATA_CONTRACT.md) (response fields, pagination, handoffs) |
| Tool quality | [`docs/MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md`](docs/MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md) (acceptance criteria for the 13 tools) |
| Local research | [`docs/LOCAL_RESEARCH_WORKFLOW.md`](docs/LOCAL_RESEARCH_WORKFLOW.md) |
| Research | [`docs/OCTOCODE_RESEARCH_MANIFEST.md`](docs/OCTOCODE_RESEARCH_MANIFEST.md) |
| Semantic assessment | [`docs/OCTOCODE_CLASIFY.md`](docs/OCTOCODE_CLASIFY.md) (contract + research loop) |
| Config | [`docs/CONFIGURATION.md`](docs/CONFIGURATION.md) · [`docs/ADDING_CONFIG.md`](docs/ADDING_CONFIG.md) (contributor guide) · [`docs/generated/CONFIG_SETTINGS.md`](docs/generated/CONFIG_SETTINGS.md) (generated — don't hand-edit) |
| Security | [`docs/SECURITY.md`](docs/SECURITY.md) |
| Release | [`docs/RELEASE.md`](docs/RELEASE.md) |
| CLI | [`packages/octocode/docs/OCTOCODE_CLI.md`](packages/octocode/docs/OCTOCODE_CLI.md) |
| Engine / LSP | [`LSP_SERVER_LIFECYCLE.md`](packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md) · [`SUPPORTED_LANGUAGES_AND_FEATURES.md`](packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md) |
| Benchmarks | [`BENCHMARK.md`](packages/octocode-benchmark/skills/octocode-benchmark/references/BENCHMARK.md) · [`SCORING.md`](packages/octocode-benchmark/skills/octocode-benchmark/references/SCORING.md) |
| Skills | Public: [`skills/README.md`](skills/README.md) · tested: [`skills-beta/`](skills-beta/) · local development: [`skills-dev/`](skills-dev/) — see [Skills](#skills) |
| Findings logs | [`.octocode/GOTCHAS.md`](.octocode/GOTCHAS.md) (raw) · [`.octocode/JEV.md`](.octocode/JEV.md) (frozen Jev history) |
