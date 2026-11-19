Load when locating a tool's layer before auditing or editing it; a fix in the wrong layer drifts from the source of truth.

# Surface map

`crates/` paths are relative to `packages/octocode-native/`, others to the monorepo root (`<repo>`). `CORE` is `../octocode-mcp-host/packages/octocode-core`.

## Layers per tool

| Layer | Owner path |
|---|---|
| Tool definition (authored) | `CORE/src/toolContract/tools/<tool>.ts`, shared builders in `CORE/src/toolContract/tools/_{fields,read,local,concepts}.ts` |
| Native rule catalog | `CORE/src/toolContract/nativeRules/` |
| Descriptions | each tool file's `description` (`<when>…</when><workflow>…</workflow>`); field `.describe()` text; `CORE/src/toolContract/published{,Forms,Json}.ts` publishes the exact admitted forms |
| MCP + CLI instructions | `CORE/src/toolContract/instructions.ts` (`MCP_INSTRUCTIONS`, plus `MCP_TOOL_INSTRUCTIONS` per available optional tool); the native CLI appends its `<cli>` line (`crates/cli/src/cli/schema.rs`) |
| Output schemas / limits | `CORE/src/toolContract/{outputSchemas,outputEnvelope,outputResponse,limits}.ts` |
| Tool contract (generated, embedded by native build.rs) | `packages/octocode-config/contract/{tool-contract.json,provenance.json,contract-fixtures.json,tool-types.schema.json,tool_types.rs,tool_types_base.rs}` Never hand-edit; regenerate with `dev contracts:regen`. |
| Field-effect review index | `crates/runtime-base/src/contracts/field-effect-coverage.json` + `crates/runtime/tests/contract_field_effects.rs` |
| Prepare / dispatch | `crates/runtime-base/src/contracts/{prepare,validate}.rs`, `crates/runtime/src/runtime/{dispatch,domain_dispatch,engine}.rs` |
| Tool implementation | `crates/tool-<family>/src/<snake_tool>/` (shared logic in `crates/tools-shared/src/`) (`clasify`: `crates/tools-shared/src/{judgment,source_capture}/`) |
| Providers / API | `crates/runtime-base/src/providers/{github,artifact,classification}/`, `crates/runtime/src/runtime/github.rs` |
| Engine primitives | `crates/engine/` |
| Caching | `crates/runtime/src/runtime/github_cache.rs`, `crates/runtime-base/src/cache/`, `crates/runtime/tests/tool_cache_contracts.rs` |
| Output shaping | `crates/runtime-base/src/response/` (pager, row_pages, rows, stage, continuations, pages, result); `crates/runtime/src/runtime/engine/channel/` |
| Cursor storage and replay | `crates/runtime-base/src/response/cursor.rs`, `crates/runtime/src/runtime/engine/{cursors,page_replay}.rs` |
| Security | `crates/runtime-base/src/security/{content,walk,scan}.rs`, `crates/runtime-base/src/policy/` |
| MCP registration | `packages/octocode-mcp/src/index.ts` launches the native server in `crates/cli/src/cli/mcp_server/` |
| CLI | `packages/octocode/src/cli/{index,native-delegate}.ts` delegates unchanged arguments; native CLI `crates/cli/src/cli/` owns parsing and rendering, with schema in `crates/cli/src/cli/schema.rs` |
| Config | `packages/octocode-config/scripts/config-contract/declaration/` (Zod) → `scripts/generate-config-contract.ts` → `config-contract.json`, `config-contract.schema.json`, `src/config/contract.generated.ts` (same package), `<repo>/docs/CONFIGURATION.md`; native struct from `crates/runtime-base/build.rs`; runtime `crates/runtime-base/src/config/` |
| Docs | `<repo>/docs/OCTOCODE_TOOLS.md`, `<repo>/docs/TOOL_DATA_CONTRACT.md`, `skills-dev/octocode-dev/docs/TOOL_QUALITY.md`, `<repo>/docs/CONFIGURATION.md`, `<repo>/docs/OCTOCODE_MCP.md`, `<repo>/docs/OCTOCODE_CLI.md` |

## Tests to read

Core contract: `CORE/src/__tests__/*` (`publishedSchemaAlignment`, `toolContracts`, `routingContracts`, `publishedSurfaceBudget`). Native: `crates/runtime/tests/runtime_{local,github,clasify,batch_response}.rs`, `crates/cli/tests/{cli_tool_contract,cli_catalog}.rs`. Replay: [tool checks](../../../packages/octocode-benchmark/CHECKS.md); `packages/octocode-mcp/tests/integration/`.

## Live views

`$OCTO schema` (enabled tools), `$OCTO schema <tool> --view query|variants` (query-row schema; required fields and examples), `$OCTO schema --view full` (catalog, instructions, grammars).

`skills-dev/octocode-dev/scripts/tool-inventory.mjs` reads the generated embed; regen first if core changed since.
