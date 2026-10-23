# Surface map

Load when locating any layer of a tool before auditing or editing it. A fix in the wrong layer drifts from the source of truth.

Paths beginning `crates/` are relative to `packages/octocode-native/`; other paths are relative to the monorepo root (`<repo>`). `CORE` is the sibling `../octocode-mcp-host/packages/octocode-core`.

## Layers per tool

| Layer | Owner path | Audit question |
|---|---|---|
| Input schema (authored) | `CORE/src/toolContract/input/resources/tools/<tool>.ts`, `CORE/src/toolContract/input/resources/tools/_toolkit.ts`, `CORE/src/toolContract/input/resources/{global,toolVariants}.ts` | Fields, defaults, bounds, variants |
| Cross-field validation | `CORE/src/toolContract/validation/<area>.ts`, `CORE/src/toolContract/nativeRules/` | Rules that native replays at prepare time |
| Descriptions | `CORE/src/toolContract/descriptions.ts` (one string per tool), `CORE/src/toolContract/publishedSchema.ts` (field notes) | Tool/field prose shown to agents |
| MCP + CLI instructions | `CORE/src/toolContract/instructions.ts` (`TOOL_RESEARCH_INSTRUCTIONS` = the MCP prompt; `buildCliInstructions` adds the `<cli>` line) | Shared workflow guidance |
| Output schemas / limits | `CORE/src/toolContract/{outputSchemas,limits}.ts` | Internal validation of produced results |
| Contract hub (re-export only) | `packages/octocode-config/src/contracts/{schema,mcp}.ts` | Must stay a thin `export *` |
| Tool contract (generated, embedded by native build.rs) | `packages/octocode-config/contract/{tool-contract.json,provenance.json,contract-fixtures.json,tool-types.schema.json,tool_types.rs}` | Never hand-edit; `yarn contracts:regen` |
| Field-effect claims | `crates/runtime/src/contracts/field-effect-coverage.json` + `crates/runtime/tests/contract_field_effects.rs` | Hand-maintained labels: verify against code |
| Prepare / dispatch | `crates/runtime/src/contracts/{prepare,validate}.rs`, `crates/runtime/src/runtime/{dispatch,domain_dispatch,engine}.rs` | Defaults, normalization, routing |
| Tool implementation | `crates/runtime/src/tools/<snake_tool>/` (`clasify` evaluates through `crates/runtime/src/tools/judgment/` and captures sources through `crates/runtime/src/tools/source_capture/`) | Business logic |
| Providers / API | `crates/runtime/src/providers/{github,artifact,classification}/`, `crates/runtime/src/runtime/github.rs` | Request count, auth, errors |
| Engine primitives | `packages/octocode-native/crates/engine/` | text search, AST, LSP, minify, secrets |
| Caching | `crates/runtime/src/runtime/github_cache.rs`, `crates/runtime/src/cache/`, `crates/runtime/src/tools/gh_clone_repo/cache.rs`, `crates/runtime/tests/tool_cache_contracts.rs` | Keys, TTL, invalidation |
| Output shaping | `crates/runtime/src/response/{pager,rows,stage,render,continuations,channels,pages,result}.rs` | Rows, evidence, `next.*` pages vs `hints` leads, compact CLI |
| Security | `crates/runtime/src/security/{content,walk,scan}.rs`, `crates/runtime/src/policy/` | Redaction, path sandbox |
| MCP registration | `packages/octocode-mcp/src/native/index.ts` (instructions), `packages/octocode-mcp/src/public.ts` | Thin forward, no logic |
| CLI | `packages/octocode/src/cli/{index,native-delegate}.ts` delegates unchanged arguments; native CLI `crates/cli/src/cli/` owns parsing and rendering, with schema in `crates/cli/src/cli/schema.rs` | Delegation, rendering, flags |
| Config | `packages/octocode-config/config-contract.json` → `packages/octocode-config/scripts/generate-config-contract.ts` → `packages/octocode-config/src/config/contract.generated.ts`, `<repo>/docs/generated/CONFIG_SETTINGS.md`; native struct from `crates/runtime/build.rs`; runtime `crates/runtime/src/config/` | One declared knob, one resolver |
| Docs | `<repo>/docs/OCTOCODE_TOOLS.md`, `<repo>/docs/TOOL_DATA_CONTRACT.md`, `skills-dev/octocode-dev/docs/TOOL_QUALITY.md`, `<repo>/docs/CONFIGURATION.md`, `<repo>/docs/OCTOCODE_MCP.md`, `<repo>/docs/OCTOCODE_CLI.md` | Match live behavior |

## Tests to read for a tool

| Scope | Where |
|---|---|
| Core contract | `CORE/src/__tests__/*` (e.g. `localDirectSchemas`, `mcpInstructions`, `routingContracts`, `agentContextBudgets`) |
| Native integration | `crates/runtime/tests/runtime_{local,github,clasify,batch_response}.rs`; native CLI `crates/cli/tests/{cli_tool_contract,cli_catalog}.rs` |
| MCP all-tool contracts | `packages/octocode-mcp/tests/tools/all-tools.pagination-contract.test.ts` |

## Live views

```bash
$OCTO schema                          # compact catalog + availability
$OCTO schema <tool> --view query       # focused fields and names of omitted fields
$OCTO schema <tool> --view full        # complete tool contract
```

`skills-dev/octocode-dev/scripts/tool-inventory.mjs` reads the generated embed, so it reflects the last regen — if core changed since, regen first or its output is stale.
