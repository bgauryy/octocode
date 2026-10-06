# AGENTS.md — Octocode Monorepo

Prefer each package's `AGENTS.md`, `ARCHITECTURE.md`, and `docs/` for its implementation. This guide holds repository rules and routing; detailed references have one owner below.

## Working tree

- **NEVER `git commit`.** The human/checkpoint bot owns commits. A background process continuously runs `git add -A`, so a commit can sweep unrelated work into the wrong change.
- **NEVER `git stash`.** It hides other sessions' work and races the checkpoint bot. Preserve concurrent edits; compare a baseline with `git show <rev>:<path>`.

## Data is quality — never trim, always paginate

**Major rule.** Tool output never drops or truncates evidence to save bytes. When a result is large, return a complete page plus an executable `next.*` continuation that reaches every remaining row, line, file, or patch exactly once; disclose any terminal limit explicitly.
- Byte reductions may only remove duplication (repeated keys, prefixes, identities, echoes) or metadata that carries no evidence. Every listed item, scanned range, match, coverage fact, and warning stays.
- A preview, clip, or summary is allowed only when the same response carries the lossless continuation to the full data (or the caller opted in to the shorter view).
- Non-evidence lists (for example skipped binary files) appear as a count plus a lossless continuation that lists every item; do not inline them and do not drop them.
- A page must fit its response window; a continuation must never skip unshown data. Silent gaps are correctness defects, not efficiency trade-offs.
- Optimize for quality × token efficiency: fewer calls and less duplication, never less evidence.

## Dogfood first

Use the local CLI, MCP, or a relevant skill before raw reads/searches: `rg`/`grep` → `localSearch`; `cat`/`head`/`sed` → `localFetch`; `ls`/`find` → `structureSearch`; symbols → `astSearch`/`lspSearch`.

```bash
OCTO='node packages/octocode/out/octocode.js'
$OCTO schema
$OCTO schema <name> --view query
$OCTO <toolName> '{"queries":[…]}'
```

Check the live schema before calling. Add `mainGoal` and `reasoning` only in multi-call research on unknowns; omit them on simple lookups. `localSearch` uses `path` + `matchString`, with `pageSize`/`matchPageSize`; it has no `operation`, `directory`, `maxResults`, `limit`, or `maxFiles`. For file discovery, `structureSearch` requires `operation:"files"` with `include` (basename or path globs) and/or `extensions`; its default operation is `tree`.

Skills are the default entry point for research, architecture, and eval work:

| Work | Entry point |
|---|---|
| Build/test/lint/typecheck/verify/docs/deps/release; contract/config changes; tool audits | [octocode-dev](skills-dev/octocode-dev/SKILL.md), load first for any development |
| Evidence, tracing, change impact | [octocode-research](skills/octocode-research/SKILL.md) |
| Architecture decisions | [octocode-architect](skills/octocode-architect/SKILL.md) |
| Benchmark/keep-discard | [octocode-eval-benchmark](skills/octocode-eval-benchmark/SKILL.md) |
| Semantic judgment affecting the next read | [octocode-research clasify gate](skills/octocode-research/references/clasify.md) |
| Local worker offload | [octocode-subagent](skills/octocode-subagent/SKILL.md) |
| Loaded context audit | [octocode-context-audit](skills-dev/octocode-context-audit/SKILL.md) |
| Open Rust/AST/LSP implementation choices | [rust-best-practices](skills-dev/rust-best-practices/SKILL.md) plus native architecture/engine docs |

Dogfood `clasify` when it changes the next action: pass unread lists/large fetches, ask relevance plus `sufficient`, and read relevant evidence that is not already answered. Its verdicts are hints; verify deciding source. Use direct search for literals. Partial coverage never proves absence.

After every tool/skill use, note friction, gaps, or bad defaults and log them instead of silently working around them. Raw findings: [GOTCHAS](.octocode/GOTCHAS.md); current classification practice: [OCTOCODE_CLASIFY](docs/OCTOCODE_CLASIFY.md); frozen history: [JEV](.octocode/JEV.md).

For repo-wide topology: `$OCTO graph ingest <path>`, then `$OCTO graph query <op>` (`$OCTO graph --help`). Graph/`astTopology` edges are candidates; confirm references/callers with `lspSearch` before deletion claims. Follow every executable `next.*` page or report the explicit terminal limit; never silently drop pages. `hints.*` leads are optional. Rebuild and test the real CLI/MCP/skill path after each package change.

## One contract pipeline

Authored `@octocodeai/octocode-core` → generated `@octocodeai/config` → native runtime + CLI/MCP/VS Code adapters. Native validates, executes, secures, and shapes tools. Interfaces contain no tool business logic, contract guidance, or TypeScript execution fallback.

- Author every tool schema, description, instruction, and limit in `../octocode-mcp-host/packages/octocode-core`.
- Build core → `yarn contracts:regen` at this root → **immediately rebuild native** → rebuild consumers. Regen refreshes the `file:` core copy via install and runs config's sole generator. It requires `cargo-typify` 0.8.0 installed with `--locked`.
- Never hand-edit `packages/octocode-config/contract/` or `src/contracts/toolTypes.generated.ts`. Native `build.rs` embeds that contract in place; no second copy/generator/pin.
- Never hand-write tool wire types: no interface Zod/TS copies or native serde query/result structs. Use generated types; accessor `impl` blocks are allowed. TS imports config `/schema` or `/mcp`, never core directly. Keep open output payloads open; tighten them in core.
- Implement new fields/discriminators and declare them in native `contracts/field-effect-coverage.json`. Public limit changes intentionally trip pinned-limit tests. Name shared/recursive vocabulary in core with `.meta({ title: "Name" })`.
- Fingerprint drift fails closed at MCP startup and CLI `schema`. Regenerate/rebuild; do not override drift for production.
- All config flows through `@octocodeai/config`; do not duplicate home/env/dotenv handling. Skills use injected `octocode-config.mjs`.
- Publish core first; `yarn workspace @octocodeai/config check:core-contract-sync:published` is a release gate.

## Build and verification

```bash
DEV='node skills-dev/octocode-dev/scripts/dev.mjs'
$DEV --help
$DEV build:dev
$DEV verify
$DEV docs:verify
```

Root package.json has no task wrappers. Use the dev skill runner for repo-wide tasks; `yarn workspace <package> <script>` remains valid for one package. Default local build is `build:dev` (debug native + TS, no clean/lint). Use `build` for release/performance artifacts. Verify exit codes, not target paths. Never lower coverage floors. Rust tests: `yarn workspace @octocodeai/octocode-native test:rust`; integration tests are one binary per crate (`tests/main.rs` declares each `tests/*.rs` as a `mod`). Do not commit local lld/sccache Cargo configuration. A single lane uses the warm repo `target/`; parallel lanes share at most 3 Cargo target dirs in total (each in a scratchpad, never `target/<name>`; Cargo's lock serializes builds in one dir; same `--features` keeps units shared), deleted when done; reset a bloated `target/` with `$DEV clean:cache` (stale native copies) or `$DEV clean` (all build outputs).

After native changes, rebuild native and affected interfaces, then exercise `$OCTO config --json`, `$OCTO schema`, and actual tool calls. Setup, dedupe, prepublish, six-platform build, and publication gates are owned by the [dev skill](skills-dev/octocode-dev/SKILL.md) and [release guide](skills-dev/octocode-dev/docs/RELEASE.md); do not publish as a side effect of local verification.

## Tools, skills, and documentation owners

The [catalog](docs/OCTOCODE_TOOLS.md) has sixteen tools; `$OCTO schema` is authoritative. [Workflows](docs/OCTOCODE_WORKFLOWS.md) maps each research flow (routes, `next` pages, `hints`, clasify, briefs) with one diagram per flow. `ghCloneRepo` and `astRewrite` are CLI-only. Topology/rewrite require `OCTOCODE_BETA=1`; classification requires `OCTOCODE_CLASSIFICATION_API`. MCP registers available read tools.

Public skills live in [skills/](skills/README.md); tested skills in `skills-beta/`; repository development skills in `skills-dev/`. `.agents/skills/` entries must be symlinks to canonical folders, never copies. Edit canonical sources.

- User docs: [docs/](docs/README.md), [protocol](docs/OCTOCODE_PROTOCOL.md), [handoffs](docs/TOOL_DATA_CONTRACT.md), [configuration](docs/CONFIGURATION.md), [authentication](docs/AUTHENTICATION.md), [security](docs/SECURITY.md). Edit source docs, not build copies or generated settings.
- Developer docs: [development](skills-dev/octocode-dev/docs/DEVELOPMENT.md), [config changes](skills-dev/octocode-dev/docs/ADDING_CONFIG.md), [tool quality](skills-dev/octocode-dev/docs/TOOL_QUALITY.md), [automation scripts](skills-dev/octocode-dev/scripts/README.md). Each package owns its architecture.
- Benchmark: [unified harness](packages/octocode-benchmark/compare/unified/README.md), [results](docs/BENCHMARKS.md). Functional suites: [octocode-local-testing](octocode-local-testing/README.md); cloned repos are not committed.

Use `npx -y` to avoid prompts and bounded deadlines for slow calls. On macOS, use `gtimeout` or `perl -e 'alarm N; exec @ARGV' -- cmd`.
