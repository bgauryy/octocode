---
name: octocode-dev
description: "Use when doing any development work inside the octocode monorepo: build, test, lint, typecheck, verify, docs checks, dependency dedupe, local dev setup, or publish (root package.json has no wrapper scripts; tasks run through this skill's dev.mjs); changing a tool contract, native runtime, CLI/MCP surface, or config setting through the one generation pipeline; and auditing or hardening a tool end to end (schema, descriptions, Rust implementation, data flow, output shape, pagination, next.* routing, config, docs drift, cleanup). Triggers: yarn build, build:dev, run tests, verify, docs:verify, prepublish, release, contracts:regen, add a config key, add a tool field, contract drift, fingerprint mismatch, stale MCP, tool audit, octocode dev. Not for researching other code → octocode-research; not for open Rust design choices → rust-best-practices."
---

# Octocode Dev

tools: `node skills-dev/octocode-dev/scripts/dev.mjs <task>` · `yarn workspace <pkg> <script>` · `node packages/octocode/out/octocode.js` (`$OCTO`) · Octocode MCP
related-skill: `octocode-research`
output: code and docs edits in the working tree; audit reports under `<repo>/.octocode/octocode-dev/`; none for plain task runs
routes: load a reference only for the step it names below; run scripts from the repo root.

Flow: `CHANGE or AUDIT → BUILD → VERIFY (real CLI/MCP path) → REPORT`

Paths: `<repo>` is the monorepo root; `CORE` is `../octocode-mcp-host/packages/octocode-core`. Bare `docs/` paths are this skill's developer docs; repository docs are always written with the `<repo>/` prefix. Use `rust-best-practices` for open native design choices and `octocode-documentation` for doc rewrites.

## Hard rules

- Never `git commit` or `git stash`; leave changes in the working tree.
- Contracts have one pipeline: edit CORE → build core → `yarn contracts:regen` → rebuild native (`yarn workspace @octocodeai/octocode-native build:dev`). Never hand-write a tool wire type, hand-edit `packages/octocode-config/contract/`, or add tool logic or guidance to an interface package.
- A new contract field or discriminator must be declared in `crates/runtime/src/contracts/field-effect-coverage.json` and implemented.
- Config flows through `@octocodeai/config`; never reimplement home, env propagation, or `.env` parsing.
- Done means verified through the real CLI or MCP path after a rebuild, not only compiled. Never lower coverage floors or special-case tests.

## Task runner — `scripts/dev.mjs`

Root `package.json` keeps only `build:native:all`, `platforms:check`, `lint:fix`, `test:quiet`, `contracts:regen`. Everything else runs here; extra arguments pass through. `--help` lists all tasks.

| Need | Command (`DEV='node skills-dev/octocode-dev/scripts/dev.mjs'`) |
|---|---|
| Fast local build (default) | `$DEV build:dev` |
| Release build (slow) | `$DEV build` |
| Tests / lint / typecheck | `$DEV test` · `$DEV lint` · `$DEV typecheck` (`:ci` variants skip benchmark) |
| Full repo contract before handoff | `$DEV verify` |
| Docs links, catalog, config keys | `$DEV docs:verify` |
| Workspace scripts present / outputs built | `$DEV health:check` · `$DEV check-outputs` |
| One range per external dependency | `$DEV deps:dedupe` (`--fix` rewrites) |
| Local dev resolutions | `$DEV setup` then `yarn install` |
| Publish guard | `$DEV prepublish` (`--fix`, `--dry-run`); CI build: `$DEV build:ci` |
| One package | `yarn workspace <pkg> <script>` |

The runner wraps these scripts; call one directly only for flags the runner does not expose:

- `scripts/workspace-health.mjs` — topo-sorted `run <script>`, `verify`, `check`, `report`, `check-outputs`.
- When debugging the docs gate, run `scripts/docs-verify.mjs` — the check behind `docs:verify`.
- When a dedupe needs flags, run `scripts/dedupe-deps.mjs` — dependency range dedupe behind `deps:dedupe`.
- When switching to local packages, run `scripts/dev-setup.mjs` — writes local `workspace:`/`file:` resolutions behind `setup`; imports `scripts/dev-resolution-contract.mjs` (shared resolution list, also used by prepublish).
- Before publishing, run `scripts/prepublish.mjs` — strips or checks local resolutions behind `prepublish`.
- `scripts/esbuild-package.mjs` — library called by the `octocode-mcp` build; not run by hand.
- `scripts/runtime-import-contract.mjs` — imported by `packages/octocode/build.mjs` to pin runtime imports; not run by hand.
- When starting a tool audit, run `scripts/tool-inventory.mjs [tool] [--json]` — audit map per tool: native module, evidence files, zero-hit and undescribed fields.

When a script flag or path rule is unclear, read `scripts/README.md`.

## Change routes — pick the row, then BUILD → VERIFY

| Changing | Do | Read |
|---|---|---|
| Tool schema, description, instruction, limit | Edit CORE → build core → `yarn contracts:regen` → native `build:dev` → declare new fields in field-effect coverage | `references/contract-audit.md`, `references/surface-map.md` |
| Native tool behavior | Edit `packages/octocode-native/crates/**`; parse rows into generated `<Tool>Query`; `yarn workspace @octocodeai/octocode-native test:rust` | `references/implementation-audit.md` |
| Output, pagination, `next.*` hints | Keep continuations executable and lossless | `references/output-audit.md`, `references/workflow-audit.md` |
| A setting or credential | Follow the config pipeline end to end | `docs/ADDING_CONFIG.md`, `references/config-docs-audit.md` |
| Package layout, build, env, ownership | — | `docs/DEVELOPMENT.md` |
| Publish / release order | Core publishes first; `$DEV prepublish --fix` → `yarn install` → `$DEV prepublish` | `docs/RELEASE.md` |
| Anything a tool returns to agents | Meet the shared acceptance bar | `docs/TOOL_QUALITY.md`, `<repo>/docs/TOOL_DATA_CONTRACT.md` |

After any native, engine, or CLI change: `yarn workspace @octocodeai/octocode-native build:dev` → `yarn workspace octocode build:dev` (or `octocode-mcp`) → `$OCTO config --json && $OCTO scheme` → call the changed tool. An MCP server started before the rebuild still serves the old contract; restart it before judging behavior. A core/native fingerprint mismatch means regenerate and rebuild, never `OCTOCODE_ALLOW_CONTRACT_DRIFT`.

## Audit a tool end to end

Use when asked to audit, harden, or clean up one tool or all tools.

1. Map: run `scripts/tool-inventory.mjs <tool>` and locate every layer with `references/surface-map.md`.
2. Lanes — load each only while working it:
   - When checking schema, descriptions, or instructions, read `references/contract-audit.md`
   - When tracing implementation and data flow, read `references/implementation-audit.md`
   - When judging output shape and pagination, read `references/output-audit.md`
   - When judging agent workflow and `next.*` routing, read `references/workflow-audit.md`
   - When checking config and docs drift, read `references/config-docs-audit.md`
3. When fixing, change the owning layer and verify with `references/fix-and-verify.md`.
4. Report with `assets/audit-report.md` into `<repo>/.octocode/octocode-dev/<tool>-<date>.md`: findings as `path:line`, severity, evidence, fix, verification command and result.

Multi-tool audits may run one subagent per tool; each gets the tool name, this skill path, and the report template.

## Done gate

- The owning layer changed, generated outputs were regenerated rather than edited, and new fields are covered.
- `$DEV build:dev` (or the package build) exited 0 and the changed path ran through `$OCTO` or MCP.
- Focused tests pass; before a handoff that spans packages, `$DEV verify` and `$DEV docs:verify` pass.
- Report the exact commands and results; name anything not verified.
