# AGENTS.md — Octocode Monorepo

Internal guide. When present, prefer each package's `ARCHITECTURE.md` / `AGENTS.md` / `docs/` over anything here.

---

## Git — hard rules

- **NEVER `git commit`.** Leave changes in the working tree; the human (or the checkpoint bot) owns commits. A background process runs `git add -A` continuously, so any `git commit` sweeps unrelated in-flight work into your commit under the wrong message. Never run it.
- **NEVER `git stash`.** It silently hides other sessions' uncommitted work and races the checkpoint bot. Use targeted reads/edits instead; to compare against a baseline, read from `git show <rev>:<path>` — do not stash.

---

## Dogfood — always, no exceptions

**This repo ships the tools. Use them on themselves.**

When working in this repo, reach for the local CLI, MCP, or a skill **first**. Measured over 30 days, agents still ran ~4,400 raw `grep`/`cat`/`find` calls against ~1,200 Octocode calls. Map them: `grep`/`rg` → `localSearch` · `cat`/`sed -n`/`head` → `localFetch` · `find`/`ls` → `structureSearch` · symbol questions → `astSearch`/`lspSearch`.

```bash
OCTO='node packages/octocode/out/octocode.js'
$OCTO scheme                                    # live catalog
$OCTO scheme <name> --compact                   # schema before calling
```

| Need | Use |
|---|---|
| Search code / files / symbols / LSP | Local CLI (`$OCTO <toolName> '<json>'`) **or** Octocode MCP |
| Repo-wide structure: callers, dependents, cycles, blast radius, possible issues | `$OCTO graph ingest <path>` once, then `$OCTO graph query <op>` (`$OCTO graph --help`) |
| GitHub code, PRs, history | same tools — `ghSearchRepo`, `ghSearchCode`, `ghStructure`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem`, `ghCloneRepo` |
| Package discovery | `artifactSearch` |
| Research / trace / change impact | `octocode-research` skill |
| Architecture decisions | `octocode-architect` skill |
| Benchmarks / keep-discard | `octocode-eval-benchmark` skill |
| Evidence-driven semantic crossroads | `octocode-clasify` skill |
| Offload bulk to local Ollama | `octocode-subagent` skill |
| Build / test / lint / verify / docs / deps / release, contract or config changes, tool audits | **`octocode-dev` skill** ([`skills-dev/octocode-dev/SKILL.md`](skills-dev/octocode-dev/SKILL.md)) — run tasks with `$DEV <task>` (see [Build](#build)) |
| After any package change | rebuild → test via real CLI/MCP/skill path — not just compile |

**Skills are first-class** — wired to the same tools; your default entry point for research, architecture, and eval flows.

**Dogfood `clasify` where it changes the next action** ([`skills/octocode-clasify/SKILL.md`](skills/octocode-clasify/SKILL.md) owns the rules): send a list or large fetch as an unread resource, ask relevance plus `sufficient`, then read only relevant items that are not already answered. Verdicts are hints; verify deciding source, and partial coverage never proves absence.

### Reflect after every tool/skill use

Note friction, gaps, or wrong defaults and log them (comment/issue) instead of silently working around — if dogfooding hurts, fix it. Raw findings → [`.octocode/GOTCHAS.md`](.octocode/GOTCHAS.md); current semantic-assessment practice → [`docs/OCTOCODE_CLASIFY.md`](docs/OCTOCODE_CLASIFY.md); frozen Jev history → [`.octocode/JEV.md`](.octocode/JEV.md).

---

## Architecture and data flow

`@octocodeai/octocode-core` (sibling repo: Zod schemas, descriptions, instructions) → `yarn contracts:regen` → `@octocodeai/config` (`./schema`, `./mcp` + generated `contract/` and TS types) → interfaces (`octocode-mcp`, `octocode` CLI, `octocode-vscode`) and the native brain (`build.rs` embeds `contract/`; crates `cli`/`runtime-napi` adapters, `runtime` validate·execute·secure·shape, `github`, `engine` ripgrep·AST·LSP·minify·secrets).

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
- Change a contract → edit core → `yarn contracts:regen` → rebuild. That is the whole change; there is no native script, copy, or pin to update. Rebuild native (`yarn workspace @octocodeai/octocode-native build:dev`, or `build` for release) right after every regen: until then the MCP server fails closed on the core/native fingerprint mismatch and new MCP sessions cannot start.
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

Full overview: [`skills-dev/octocode-dev/docs/DEVELOPMENT.md`](skills-dev/octocode-dev/docs/DEVELOPMENT.md); each package has its own `ARCHITECTURE.md` and `README.md`.

| Role | Packages |
|---|---|
| Core | `octocode-config` (`@octocodeai/config`: env/config loader + the single contract generator) · `octocode-native` (Rust brain: runtime, GitHub, CLI hosts, N-API, engine) · external `@octocodeai/octocode-core` (authors all tool contracts) |
| Interfaces | `octocode-mcp` (thin MCP stdio server) · `octocode` (CLI: `<toolName> '<json>'`, `scheme`, `skill`, `config`, `login`/`logout`/`auth`, `install`) · `octocode-vscode` |
| Support (private) | `octocode-skill-installer` · `octocode-agents-communication` (in `skills/`) · `octocode-benchmark` (agent-vs-agent eval) |

Local core changes → build core → `yarn contracts:regen` → rebuild consumers.

---

## Tools

16 catalog tools. Full reference: [`docs/OCTOCODE_TOOLS.md`](docs/OCTOCODE_TOOLS.md) · handoffs: [`docs/TOOL_DATA_CONTRACT.md`](docs/TOOL_DATA_CONTRACT.md) · live: `$OCTO scheme`. `ghCloneRepo` and `astRewrite` are CLI-only; MCP registers the rest when their availability gates pass.

| Family | Tools | Role |
|---|---|---|
| GitHub | `ghSearchRepo` · `ghSearchCode` · `ghStructure` · `ghGetFileContent` · `ghSearchHistory` · `ghGetHistoryItem` · `ghCloneRepo` | Code/repo/tree discovery, exact reads, history, clone |
| Package | `artifactSearch` | Lookup across 8 ecosystems + source repo |
| Local | `localSearch` · `structureSearch` · `localFetch` | Text/regex search · directory tree/file discovery (no parser) · exact/minified file reads |
| AST | `astSearch` | Syntax trees, symbols, structural match |
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
| [`skills/`](skills/) | **Public skills.** Published with the CLI. `octocode skill install <name>` installs from here. |
| [`skills-beta/`](skills-beta/) | **Tested skills.** Not published and not bundled. |
| [`skills-dev/`](skills-dev/) | **Local development.** For working on this repository. Not published. |

### Public — [`skills/`](skills/)

Published with the CLI; the catalog with "use when" lines is [`skills/README.md`](skills/README.md). Communication runtime lives in [`skills/octocode-agents-communication/`](skills/octocode-agents-communication/).

### Tested — [`skills-beta/`](skills-beta/) (not published)

| Skill | Use when |
|---|---|
| [`octocode-architecture-view`](skills-beta/octocode-architecture-view/) | A system's architecture should be an interactive HTML map: layers, modules, dependencies, runtime flows, data stores |

### Local development — [`skills-dev/`](skills-dev/) (not published; for working on this repository)

| Skill | Use when |
|---|---|
| [`octocode-dev`](skills-dev/octocode-dev/) | **Any dev work in this repo** — load it first. Owns the task runner (`scripts/dev.mjs`, replaces root `package.json` scripts), all repo automation scripts, the change pipeline (contract → native → CLI/MCP → config → docs → release), developer docs (`docs/DEVELOPMENT.md`, `ADDING_CONFIG.md`, `TOOL_QUALITY.md`, `RELEASE.md`), and end-to-end tool audits (`scripts/tool-inventory.mjs`; reports under `.octocode/octocode-dev/`) |
| [`octocode-context-audit`](skills-dev/octocode-context-audit/) | Agent context feels bloated, or before adding instructions/skills/MCP servers: measures what loads every session vs what is used |
| [`rust-best-practices`](skills-dev/rust-best-practices/) | Rust choices are open: crates, error shape, module/workspace layout, cargo profiles, deps |
For AST/LSP implementation work, use `rust-best-practices` with the native package's architecture and engine docs. Record verified engine defects in `.octocode/GOTCHAS.md` and the owning package documentation.

---

## Build

Repo-wide tasks run through the **`octocode-dev` skill**, not root `package.json` scripts (they were removed). Load [`skills-dev/octocode-dev/SKILL.md`](skills-dev/octocode-dev/SKILL.md) for change routes, regen order, and verification gates.

```bash
DEV='node skills-dev/octocode-dev/scripts/dev.mjs'
$DEV --help                                         # every task, one line each
$DEV build:dev                                      # FAST local full build (debug) — the default
$DEV build                                          # all packages, RELEASE (slow — see below)
$DEV test · $DEV lint · $DEV typecheck · $DEV verify
$DEV docs:verify · $DEV health:check · $DEV deps:dedupe [--fix]
yarn workspace <pkg-name> <script>                  # single package (package scripts are unchanged)
yarn build:native:all · yarn platforms:check        # 6-platform cross-compile (publish only)
yarn contracts:regen                                # core → contract/ regen
```

CI calls the same tasks (`.github/workflows/ci.yml`). **Use `$DEV build:dev` locally** (debug native + TS); reserve `$DEV build` (release) for release/perf-representative artifacts. Build internals (parallel workspace graph, concurrent native targets, profiles) live in [`packages/octocode-native/ARCHITECTURE.md`](packages/octocode-native/ARCHITECTURE.md). Do **not** commit `.cargo/config.toml` lld/sccache blocks.

**End-to-end after engine/native/CLI changes:**

```bash
yarn workspace @octocodeai/octocode-native build:dev
yarn workspace octocode build:dev        # or: yarn workspace octocode-mcp build:dev
$OCTO config --json && $OCTO scheme
```

`build:dev` skips clean + lint and builds the CLI and both addons in debug mode. Verify by exit code — don't inspect `target/debug/` paths. Coverage floors are per-package ratchets in `vitest.config.*` — never lower them, raise when coverage improves. Rust tests: `yarn workspace @octocodeai/octocode-native test:rust`.

---

## Dev setup / publish

```bash
$DEV setup && yarn install       # local dev: resolve internal packages from workspace
```

**Before publishing:**

```bash
$DEV prepublish --fix               # strip local workspace: resolutions
yarn install && $DEV prepublish     # lockfile + final guard
```

`$DEV prepublish` can also `--dry-run` (preview) or run without flags (check only). Packages version independently; core publishes **before** packages that embed its contracts. Full gated order: [`skills-dev/octocode-dev/docs/RELEASE.md`](skills-dev/octocode-dev/docs/RELEASE.md) · scripts: [`skills-dev/octocode-dev/scripts/README.md`](skills-dev/octocode-dev/scripts/README.md).

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

Two homes, one owner per topic:
- **User docs** — [`docs/`](docs/README.md) (edit here; `out/docs` copies are build output). Start with [protocol](docs/OCTOCODE_PROTOCOL.md) (concepts, research loop, why it works) and [benchmarks](docs/BENCHMARKS.md) (agent-vs-agent results). Reference: [tools](docs/OCTOCODE_TOOLS.md) · [response fields and handoffs](docs/TOOL_DATA_CONTRACT.md) · [clasify](docs/OCTOCODE_CLASIFY.md) · [configuration](docs/CONFIGURATION.md) (generated settings: `docs/generated/CONFIG_SETTINGS.md`, never hand-edit) · [authentication](docs/AUTHENTICATION.md) · [security](docs/SECURITY.md).
- **Developer docs** — owned by the [`octocode-dev`](skills-dev/octocode-dev/SKILL.md) skill in [`skills-dev/octocode-dev/docs/`](skills-dev/octocode-dev/docs/DEVELOPMENT.md): [development](skills-dev/octocode-dev/docs/DEVELOPMENT.md) (packages, contract pipeline, build/test) · [adding config](skills-dev/octocode-dev/docs/ADDING_CONFIG.md) · [tool quality bar](skills-dev/octocode-dev/docs/TOOL_QUALITY.md) · [release](skills-dev/octocode-dev/docs/RELEASE.md). Scripts and the task runner: [`skills-dev/octocode-dev/scripts/`](skills-dev/octocode-dev/scripts/README.md).
- **Benchmark and harness** — agent-vs-agent eval (Octocode MCP + clasify vs `rg` + `gh`): [`packages/octocode-benchmark/eval/`](packages/octocode-benchmark/README.md), results in [benchmarks](docs/BENCHMARKS.md). Regression suites and validation reports: [`octocode-local-testing/`](octocode-local-testing/README.md) (`harness/`, `validate/*/REPORT.md`); cloned repos are not committed ([repos/README.md](octocode-local-testing/repos/README.md)).

Findings logs: [`.octocode/GOTCHAS.md`](.octocode/GOTCHAS.md) (raw) · [`.octocode/JEV.md`](.octocode/JEV.md) (frozen).
