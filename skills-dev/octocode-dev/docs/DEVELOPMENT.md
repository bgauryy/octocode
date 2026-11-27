Load when locating the code that owns a behavior, or changing a tool schema, description, instruction or limit through the contract pipeline.

# Developing Octocode

Where code lives and the contract pipeline. Repo rules and the package map: `<repo>/AGENTS.md`. `$DEV` and `$OCTO` are defined there; run from the repo root.

- Cargo builds, build slots, pruning, [Rust dev tools](RUST_BUILDS.md#rust-dev-tools), development environment variables: [RUST_BUILDS.md](RUST_BUILDS.md)
- oxlint, workspace scripts, the clippy ratchet, recorded provider answers: [LINT_CHECKS.md](LINT_CHECKS.md)
- Profiling a tool flow: [PROFILING.md](PROFILING.md)

## Where code lives

`crates/` is `packages/octocode-native/crates/`; `CORE` is `../octocode-mcp-host/packages/octocode-core`. A tool crate imports no other tool; code two tools share goes in `tools-shared`, layered by `SHARED_LAYERS` (`crates/runtime/src/tools/module_boundaries/tool_edges.rs`). `engine` holds primitives (text search, tree-sitter, LSP client pool, minification, secret scanning, code graph), never tool policy; `io` holds I/O and HTTP primitives; `test-support` holds fixtures no release build links (a dev-dependency only).

| Piece | Path |
|---|---|
| Tool definition, description, field `.describe()` text | `CORE/src/toolContract/tools/<tool>.ts`; shared builders `tools/_*.ts`; published forms `published{,Forms,Json}.ts` |
| Native rule catalog | `CORE/src/toolContract/nativeRules/` |
| Instructions | `CORE/src/toolContract/instructions.ts` (`MCP_INSTRUCTIONS`, plus `MCP_TOOL_INSTRUCTIONS` per optional tool); the CLI appends its `<cli>` line in `crates/cli/src/cli/schema.rs` |
| Output schemas, public limits | `CORE/src/toolContract/{outputSchemas,outputEnvelope,outputResponse,limits}.ts` |
| Generated contract and wire types | `packages/octocode-config/contract/`, embedded by `crates/runtime-base/build.rs`; types in `contract-types`, `contract-base` |
| Field-effect index | `crates/runtime-base/src/contracts/field-effect-coverage.json`, gated by `crates/runtime/tests/field_effect_coverage.rs` |
| Validation | `crates/runtime-base/src/contracts/` (`prepare.rs`, `validate.rs`, `normalize.rs`) |
| Catalog, admission, dispatch, cursors, page replay | `crates/runtime/src/runtime/` (`dispatch.rs`, `domain_dispatch.rs`, `engine/`) |
| Tool code | `crates/tool-<family>/src/<snake_tool>/` (`local`, `ast`, `lsp`, `github`, `packages`, `graph`, `clasify`); clasify judgment in `crates/tools-shared/src/{judgment,source_capture}/` |
| GitHub transport, quota budget, answer reuse | `crates/github/` (`transport/saved.rs` decides when a saved answer may answer again) |
| Providers, saved-answer store | `crates/runtime-base/src/providers/` |
| Response shaping, pages, continuations, partial marks | `crates/runtime-base/src/response/` (`paging.rs`, `continuations.rs`, `cursor.rs`) |
| Path sandbox, redaction | `crates/runtime-base/src/{policy,security}/`, `crates/engine/src/security/` |
| Internal limits | `crates/runtime-base/src/execution/limits.rs` (`RuntimeLimits::DEFAULT`) |
| CLI and stdio MCP server | `crates/cli/src/cli.rs`, `crates/cli/src/cli/mcp_server*`; launchers `packages/octocode/src/cli/native-delegate.ts`, `packages/octocode-mcp/src/index.ts` |
| Skill commands | `crates/cli/src/cli/skill/`; its build script embeds public `skills/` through the Node staging filter (Node needed at build time) |
| Settings | [ADDING_CONFIG.md](ADDING_CONFIG.md) |
| Loopback providers, answer recorder | `skills-dev/octocode-dev/scripts/providers/` |
| Tests | core `CORE/src/__tests__/` (`publishedSurfaceBudget` holds the context cost); native `crates/runtime/tests/`, `crates/cli/tests/` (each crate's integration tests are one binary: `tests/main.rs` declares every file as a `mod`) |

## Contract pipeline

1. Author in core (Zod schema, description, instructions, limits) and build core.
2. `$DEV contracts:regen` refreshes the configured core (the sibling during local development, the npm pin after `$DEV setup --reset`) and runs `@octocodeai/config generate:tool-contract`, the only generator. `check:tool-contract` fails on a stale `contract/`.
3. `$DEV build:dev`. Before replacing a runnable host executable, staging compares its `--version` with the package version and the first eight characters of the contract fingerprint; on a mismatch the old executable stays.

`tool_types.rs` holds only the generated types native source names and what they reach: after a native edit that starts or stops naming one, regenerate. The binary embeds the contract it serves; there is no runtime override.

A new tool field: add it in core and regenerate, implement it, then declare its effect or let it inherit its nearest declared ancestor's (`$DEV test:dev -- field_effect_coverage::` fails on an unresolved path or a stale key). A public limit lives in core `limits` and reaches native through the contract. Update `<repo>/docs/OCTOCODE_TOOLS.md`.
